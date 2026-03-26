use std::sync::Arc;

use lv_core::uri::VikingUri;
use lv_core::CoreError;

use super::markdown::parse_markdown;
use crate::service::context::RequestContext;
use crate::storage::{Database, EmbeddingQueue, EmbeddingTask, VikingFs};

#[derive(Debug)]
pub struct ImportRequest {
    /// Source: URL (http/https) or local file path
    pub source: String,
    /// Target URI (e.g., "viking://resources/my-doc")
    pub target_uri: Option<String>,
    /// Wait for processing to complete
    pub wait: bool,
}

#[derive(Debug)]
pub struct ImportResult {
    pub root_uri: String,
    pub nodes_created: usize,
    pub processing_queued: usize,
}

pub struct ImportPipeline {
    #[allow(dead_code)]
    db: Arc<Database>,
    viking_fs: VikingFs,
    embedding_queue: EmbeddingQueue,
}

impl ImportPipeline {
    pub fn new(db: Arc<Database>, embedding_queue: EmbeddingQueue) -> Self {
        let viking_fs = VikingFs::new(Arc::clone(&db));
        Self {
            db,
            viking_fs,
            embedding_queue,
        }
    }

    pub fn embedding_queue(&self) -> &EmbeddingQueue {
        &self.embedding_queue
    }

    pub async fn import(
        &self,
        req: &ImportRequest,
        ctx: &RequestContext,
    ) -> Result<ImportResult, CoreError> {
        // 1. Fetch content
        let (content, filename) = self.fetch(&req.source).await?;

        // 2. Determine root URI
        let root_name = req
            .target_uri
            .clone()
            .unwrap_or_else(|| format!("viking://resources/{}", slug_from_filename(&filename)));
        let root_uri = VikingUri::parse(&root_name)?;

        // 3. Parse content (markdown for now)
        let nodes = parse_markdown(&content);

        // 4. Create root directory
        self.viking_fs.mkdir(&root_uri, &ctx.owner)?;

        // 5. Write tree to storage + queue embeddings
        let mut nodes_created = 0;
        let mut processing_queued = 0;

        for node in &nodes {
            let child_uri = root_uri.child(&node.slug);

            // Write context + content
            self.viking_fs
                .write_context(&child_uri, "", "", true, &ctx.owner)?;
            self.viking_fs.write_content_raw(&child_uri, &node.text)?;

            nodes_created += 1;

            // Queue L0 generation + embedding
            self.embedding_queue
                .enqueue(EmbeddingTask {
                    uri: child_uri.as_str().to_string(),
                    level: 0,
                    text: node.text.clone(),
                })
                .await?;
            processing_queued += 1;
        }

        // Also queue L0 generation for the root (from concatenated child abstracts)
        // This will be regenerated once children are processed
        let root_text = format!(
            "Directory containing {} sections: {}",
            nodes.len(),
            nodes
                .iter()
                .map(|n| n.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        self.embedding_queue
            .enqueue(EmbeddingTask {
                uri: root_uri.as_str().to_string(),
                level: 0,
                text: root_text,
            })
            .await?;
        processing_queued += 1;

        // 6. Optionally wait for all processing
        if req.wait {
            self.embedding_queue.flush().await;
        }

        Ok(ImportResult {
            root_uri: root_uri.as_str().to_string(),
            nodes_created,
            processing_queued,
        })
    }

    async fn fetch(&self, source: &str) -> Result<(String, String), CoreError> {
        if source.starts_with("http://") || source.starts_with("https://") {
            let resp = reqwest::get(source)
                .await
                .map_err(|e| CoreError::Internal(format!("fetch URL failed: {e}")))?;

            if !resp.status().is_success() {
                return Err(CoreError::Internal(format!(
                    "fetch URL failed: HTTP {}",
                    resp.status()
                )));
            }

            let filename = source.rsplit('/').next().unwrap_or("resource").to_string();

            let content = resp
                .text()
                .await
                .map_err(|e| CoreError::Internal(format!("read URL body failed: {e}")))?;

            Ok((content, filename))
        } else {
            // Local file
            let content = std::fs::read_to_string(source)
                .map_err(|e| CoreError::Internal(format!("read file failed: {e}")))?;

            let filename = std::path::Path::new(source)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("resource")
                .to_string();

            Ok((content, filename))
        }
    }
}

fn slug_from_filename(filename: &str) -> String {
    let stem = filename.rsplit('.').next_back().unwrap_or(filename);
    stem.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
