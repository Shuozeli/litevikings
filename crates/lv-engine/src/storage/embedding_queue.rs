use std::sync::Arc;
use std::time::Duration;

use duckdb::Connection;
use lv_core::CoreError;
use tokio::sync::mpsc;
use tracing;

use crate::llm::prompts;
use crate::llm::{ChatMessage, ChatModel, ChatRequest, Embedder};

/// Max characters to send to LLM for L0 generation.
/// Prevents OOM/timeout on large documents.
const MAX_LLM_INPUT_CHARS: usize = 4000;

/// Timeout for a single LLM call.
const LLM_TIMEOUT: Duration = Duration::from_secs(30);

/// A task to generate L0/L1 and compute embedding for a context.
#[derive(Debug)]
pub struct EmbeddingTask {
    pub uri: String,
    pub level: i32,
    pub text: String,
}

/// Async background worker that processes embedding tasks.
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

        // Spawn on a dedicated OS thread with its own tokio runtime.
        // This avoids deadlocking the main runtime when blocking on LLM calls.
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
                Some(task) = rx.recv() => {
                    match Self::process_task(&conn, &*embedder, &*chat, &task).await {
                        Ok(()) => {
                            processed += 1;
                            if processed.is_multiple_of(10) {
                                tracing::info!(
                                    processed = processed,
                                    errors = errors,
                                    pending = rx.len(),
                                    "embedding progress"
                                );
                            }
                        }
                        Err(e) => {
                            errors += 1;
                            tracing::warn!(
                                uri = %task.uri,
                                error = %e,
                                "embedding task failed, skipping"
                            );
                        }
                    }
                }
                Some(done_tx) = flush_rx.recv() => {
                    // Drain remaining tasks
                    let pending = rx.len();
                    tracing::info!(pending = pending, "flush: draining remaining tasks");
                    while let Ok(task) = rx.try_recv() {
                        match Self::process_task(&conn, &*embedder, &*chat, &task).await {
                            Ok(()) => processed += 1,
                            Err(e) => {
                                errors += 1;
                                tracing::warn!(uri = %task.uri, error = %e, "embedding task failed during flush");
                            }
                        }
                        if processed.is_multiple_of(10) {
                            tracing::info!(processed = processed, errors = errors, "flush progress");
                        }
                    }
                    tracing::info!(processed = processed, errors = errors, "flush complete");
                    let _ = done_tx.send(());
                }
                else => break,
            }
        }
        tracing::info!(
            processed = processed,
            errors = errors,
            "embedding worker exiting"
        );
    }

    async fn process_task(
        conn: &Connection,
        embedder: &dyn Embedder,
        chat: &dyn ChatModel,
        task: &EmbeddingTask,
    ) -> Result<(), CoreError> {
        // 1. Generate L0 abstract (with truncation + timeout)
        let abstract_text = if task.level == 0 && task.text.len() > 200 {
            let truncated = truncate_text(&task.text, MAX_LLM_INPUT_CHARS);
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
                    tracing::debug!(uri = %task.uri, error = %e, "L0 generation failed, using truncated text");
                    truncated
                }
                Err(_) => {
                    tracing::debug!(uri = %task.uri, "L0 generation timed out, using truncated text");
                    truncate_text(&task.text, 200)
                }
            }
        } else {
            task.text.clone()
        };

        // 2. Compute embedding (with timeout)
        let embed_text = truncate_text(&abstract_text, 2000);
        let vector = match tokio::time::timeout(
            Duration::from_secs(10),
            embedder.embed(&embed_text),
        )
        .await
        {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => return Err(CoreError::Internal(format!("embedding failed: {e}"))),
            Err(_) => return Err(CoreError::Internal("embedding timed out".to_string())),
        };

        // 3. Write abstract + vector to DB
        conn.execute(
            "UPDATE contexts SET abstract_text = $1, updated_at = now()
             WHERE uri = $2 AND level = $3",
            duckdb::params![abstract_text, task.uri, task.level],
        )
        .map_err(|e| CoreError::Internal(format!("update abstract: {e}")))?;

        let vec_str = vector
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE contexts SET vector = [{vec_str}], updated_at = now()
             WHERE uri = $1 AND level = $2"
        );
        conn.execute(&sql, duckdb::params![task.uri, task.level])
            .map_err(|e| CoreError::Internal(format!("update vector: {e}")))?;

        Ok(())
    }
}

/// Truncate text to approximately `max_chars`, breaking at a char boundary and sentence end.
fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        return text.to_string();
    }
    // Find the last char boundary at or before max_chars
    let mut end = max_chars.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let truncated = &text[..end];
    // Try to break at last sentence end
    if let Some(pos) = truncated.rfind(". ") {
        truncated[..=pos].to_string()
    } else if let Some(pos) = truncated.rfind('\n') {
        truncated[..pos].to_string()
    } else {
        truncated.to_string()
    }
}
