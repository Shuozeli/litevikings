use std::sync::Arc;
use std::time::Duration;

use duckdb::Connection;
use lv_core::CoreError;
use tokio::sync::mpsc;
use tracing;

use crate::llm::prompts;
use crate::llm::{ChatMessage, ChatModel, ChatRequest, Embedder};

/// Max characters to send to LLM for L0 generation.
const MAX_LLM_INPUT_CHARS: usize = 4000;

/// Max characters to send to embedding model.
const MAX_EMBED_CHARS: usize = 2000;

/// Timeout for a single LLM call.
const LLM_TIMEOUT: Duration = Duration::from_secs(30);

/// Number of texts to batch per embedding API call.
const EMBED_BATCH_SIZE: usize = 20;

/// How long to wait collecting a batch before sending what we have.
const BATCH_COLLECT_TIMEOUT: Duration = Duration::from_millis(500);

/// A task to generate L0/L1 and compute embedding for a context.
#[derive(Debug)]
pub struct EmbeddingTask {
    pub uri: String,
    pub level: i32,
    pub text: String,
}

/// Async background worker that processes embedding tasks in batches.
#[derive(Clone)]
pub struct EmbeddingQueue {
    tx: mpsc::Sender<EmbeddingTask>,
    flush_tx: mpsc::Sender<tokio::sync::oneshot::Sender<()>>,
}

impl EmbeddingQueue {
    /// Spawn the background worker on a dedicated thread with its own runtime.
    pub fn spawn(
        worker_conn: Connection,
        embedder: Arc<dyn Embedder>,
        chat: Arc<dyn ChatModel>,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<EmbeddingTask>(4096);
        let (flush_tx, flush_rx) = mpsc::channel::<tokio::sync::oneshot::Sender<()>>(16);

        let handle = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build embedding worker runtime");

            rt.block_on(Self::worker_loop(rx, flush_rx, worker_conn, embedder, chat));
        });

        (Self { tx, flush_tx }, handle)
    }

    pub async fn enqueue(&self, task: EmbeddingTask) -> Result<(), CoreError> {
        self.tx
            .send(task)
            .await
            .map_err(|e| CoreError::Internal(format!("embedding queue send failed: {e}")))
    }

    pub async fn flush(&self) {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        if self.flush_tx.send(done_tx).await.is_ok() {
            let _ = done_rx.await;
        }
    }

    async fn worker_loop(
        mut rx: mpsc::Receiver<EmbeddingTask>,
        mut flush_rx: mpsc::Receiver<tokio::sync::oneshot::Sender<()>>,
        conn: Connection,
        embedder: Arc<dyn Embedder>,
        chat: Arc<dyn ChatModel>,
    ) {
        let mut processed: u64 = 0;
        let mut errors: u64 = 0;

        loop {
            tokio::select! {
                Some(first_task) = rx.recv() => {
                    // Collect a batch: start with the task we just received
                    let mut batch = vec![first_task];

                    // Try to fill the batch up to EMBED_BATCH_SIZE, with a short timeout
                    let deadline = tokio::time::Instant::now() + BATCH_COLLECT_TIMEOUT;
                    while batch.len() < EMBED_BATCH_SIZE {
                        match tokio::time::timeout_at(deadline, rx.recv()).await {
                            Ok(Some(task)) => batch.push(task),
                            _ => break, // timeout or channel closed
                        }
                    }

                    let batch_size = batch.len();
                    match Self::process_batch(&conn, &*embedder, &*chat, batch).await {
                        Ok(batch_errors) => {
                            processed += batch_size as u64;
                            errors += batch_errors;
                        }
                        Err(e) => {
                            errors += batch_size as u64;
                            tracing::error!(error = %e, batch_size, "entire batch failed");
                        }
                    }

                    if processed.is_multiple_of(10) || batch_size > 1 {
                        tracing::info!(
                            processed, errors,
                            pending = rx.len(),
                            batch_size,
                            "embedding progress"
                        );
                    }
                }
                Some(done_tx) = flush_rx.recv() => {
                    // Drain all remaining tasks in batches
                    let pending = rx.len();
                    tracing::info!(pending, "flush: draining remaining tasks");

                    let mut batch = Vec::new();
                    while let Ok(task) = rx.try_recv() {
                        batch.push(task);
                        if batch.len() >= EMBED_BATCH_SIZE {
                            let batch_size = batch.len();
                            match Self::process_batch(&conn, &*embedder, &*chat, std::mem::take(&mut batch)).await {
                                Ok(batch_errors) => {
                                    processed += batch_size as u64;
                                    errors += batch_errors;
                                }
                                Err(e) => {
                                    errors += batch_size as u64;
                                    tracing::error!(error = %e, "flush batch failed");
                                }
                            }
                            if processed.is_multiple_of(10) {
                                tracing::info!(processed, errors, "flush progress");
                            }
                        }
                    }
                    // Process remaining partial batch
                    if !batch.is_empty() {
                        let batch_size = batch.len();
                        match Self::process_batch(&conn, &*embedder, &*chat, batch).await {
                            Ok(batch_errors) => {
                                processed += batch_size as u64;
                                errors += batch_errors;
                            }
                            Err(e) => {
                                errors += batch_size as u64;
                                tracing::error!(error = %e, "flush final batch failed");
                            }
                        }
                    }
                    tracing::info!(processed, errors, "flush complete");
                    let _ = done_tx.send(());
                }
                else => break,
            }
        }
        tracing::info!(processed, errors, "embedding worker exiting");
    }

    /// Process a batch of tasks: generate L0 abstracts, batch-embed, write to DB.
    async fn process_batch(
        conn: &Connection,
        embedder: &dyn Embedder,
        chat: &dyn ChatModel,
        tasks: Vec<EmbeddingTask>,
    ) -> Result<u64, CoreError> {
        let mut errors = 0u64;

        // Phase 1: Generate L0 abstracts for each task (sequential LLM calls)
        let mut abstracts: Vec<String> = Vec::with_capacity(tasks.len());
        for task in &tasks {
            let abstract_text = if task.level == 0 && task.text.len() > 200 {
                generate_l0(chat, &task.text, &task.uri).await
            } else {
                task.text.clone()
            };
            abstracts.push(abstract_text);
        }

        // Phase 2: Batch embed all abstracts in one API call
        let embed_texts: Vec<String> = abstracts
            .iter()
            .map(|a| truncate_text(a, MAX_EMBED_CHARS))
            .collect();

        let vectors = match tokio::time::timeout(
            Duration::from_secs(30),
            embedder.embed_batch(&embed_texts),
        )
        .await
        {
            Ok(Ok(vecs)) => vecs,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, batch_size = tasks.len(), "batch embedding failed, falling back to individual");
                // Fallback: try embedding one at a time
                let mut vecs = Vec::new();
                for text in &embed_texts {
                    match embedder.embed(text).await {
                        Ok(v) => vecs.push(v),
                        Err(e) => {
                            tracing::warn!(error = %e, "individual embedding failed");
                            vecs.push(vec![]); // empty vector = skip
                            errors += 1;
                        }
                    }
                }
                vecs
            }
            Err(_) => {
                tracing::warn!(batch_size = tasks.len(), "batch embedding timed out");
                return Err(CoreError::Internal("batch embedding timed out".to_string()));
            }
        };

        // Phase 3: Write abstracts + vectors to DB
        for (i, task) in tasks.iter().enumerate() {
            let abstract_text = &abstracts[i];
            let vector = vectors.get(i);

            // Skip if embedding failed (empty vector)
            if vector.is_none_or(|v| v.is_empty()) {
                errors += 1;
                continue;
            }
            let vector = vector.unwrap();

            // Write abstract
            if let Err(e) = conn.execute(
                "UPDATE contexts SET abstract_text = $1, updated_at = now()
                 WHERE uri = $2 AND level = $3",
                duckdb::params![abstract_text, task.uri, task.level],
            ) {
                tracing::warn!(uri = %task.uri, error = %e, "update abstract failed");
                errors += 1;
                continue;
            }

            // Write vector
            let vec_str = vector
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "UPDATE contexts SET vector = [{vec_str}], updated_at = now()
                 WHERE uri = $1 AND level = $2"
            );
            if let Err(e) = conn.execute(&sql, duckdb::params![task.uri, task.level]) {
                tracing::warn!(uri = %task.uri, error = %e, "update vector failed");
                errors += 1;
            }
        }

        Ok(errors)
    }
}

/// Generate L0 abstract via LLM with truncation and timeout.
async fn generate_l0(chat: &dyn ChatModel, text: &str, uri: &str) -> String {
    let truncated = truncate_text(text, MAX_LLM_INPUT_CHARS);
    let prompt = prompts::GENERATE_ABSTRACT.replace("{content}", &truncated);

    match tokio::time::timeout(
        LLM_TIMEOUT,
        chat.complete(ChatRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                text: prompt,
            }],
            temperature: 0.3,
            max_tokens: Some(256),
        }),
    )
    .await
    {
        Ok(Ok(resp)) => resp.content,
        Ok(Err(e)) => {
            tracing::debug!(uri, error = %e, "L0 generation failed, using truncated text");
            truncated
        }
        Err(_) => {
            tracing::debug!(uri, "L0 generation timed out, using truncated text");
            truncate_text(text, 200)
        }
    }
}

/// Truncate text to approximately `max_chars`, breaking at a char boundary and sentence end.
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    let mut end = max_chars.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    if let Some(pos) = truncated.rfind(". ") {
        truncated[..=pos].to_string()
    } else if let Some(pos) = truncated.rfind('\n') {
        truncated[..pos].to_string()
    } else {
        truncated.to_string()
    }
}
