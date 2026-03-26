use std::sync::Arc;

use lv_core::uri::VikingUri;
use lv_core::CoreError;

use super::context::RequestContext;
use crate::storage::{Database, DirEntry, EmbeddingQueue, EmbeddingTask, LsOptions, VikingFs};

/// Filesystem operations service. Maps to upstream FSService.
pub struct FSService {
    viking_fs: VikingFs,
    embedding_queue: Option<EmbeddingQueue>,
}

impl FSService {
    pub fn new(db: Arc<Database>, embedding_queue: Option<EmbeddingQueue>) -> Self {
        Self {
            viking_fs: VikingFs::new(db),
            embedding_queue,
        }
    }

    pub fn ls(
        &self,
        uri: &str,
        _ctx: &RequestContext,
        opts: &LsOptions,
    ) -> Result<Vec<DirEntry>, CoreError> {
        let parsed = VikingUri::parse(uri)?;
        self.viking_fs.ls(&parsed, opts)
    }

    pub fn mkdir(&self, uri: &str, ctx: &RequestContext) -> Result<(), CoreError> {
        let parsed = VikingUri::parse(uri)?;
        self.viking_fs.mkdir(&parsed, &ctx.owner)
    }

    pub fn rm(&self, uri: &str, _ctx: &RequestContext, recursive: bool) -> Result<u64, CoreError> {
        let parsed = VikingUri::parse(uri)?;
        self.viking_fs.rm(&parsed, recursive)
    }

    pub fn mv(&self, from: &str, to: &str, _ctx: &RequestContext) -> Result<(), CoreError> {
        let from_parsed = VikingUri::parse(from)?;
        let to_parsed = VikingUri::parse(to)?;
        self.viking_fs.mv(&from_parsed, &to_parsed)
    }

    pub fn stat(&self, uri: &str, _ctx: &RequestContext) -> Result<StatResult, CoreError> {
        let parsed = VikingUri::parse(uri)?;
        let exists = self.viking_fs.exists(&parsed)?;
        if !exists {
            return Err(CoreError::NotFound(format!("{uri} not found")));
        }
        let children = self.viking_fs.ls(
            &parsed,
            &LsOptions {
                node_limit: 10000,
                ..Default::default()
            },
        )?;
        let abstract_text = self.viking_fs.read_abstract(&parsed).unwrap_or_default();

        Ok(StatResult {
            uri: uri.to_string(),
            is_leaf: children.is_empty(),
            context_type: parsed.derive_context_type().as_str().to_string(),
            abstract_text,
            child_count: children.len() as i64,
        })
    }

    pub fn read(&self, uri: &str, _ctx: &RequestContext) -> Result<String, CoreError> {
        let parsed = VikingUri::parse(uri)?;
        self.viking_fs.read_content(&parsed)
    }

    pub fn read_abstract(&self, uri: &str, _ctx: &RequestContext) -> Result<String, CoreError> {
        let parsed = VikingUri::parse(uri)?;
        self.viking_fs.read_abstract(&parsed)
    }

    pub fn read_overview(&self, uri: &str, _ctx: &RequestContext) -> Result<String, CoreError> {
        let parsed = VikingUri::parse(uri)?;
        self.viking_fs.read_overview(&parsed)
    }

    /// Write content and trigger L0 generation + embedding via the queue.
    pub async fn write(
        &self,
        uri: &str,
        content: &str,
        ctx: &RequestContext,
    ) -> Result<(), CoreError> {
        let parsed = VikingUri::parse(uri)?;

        // Write the context node + content blob
        self.viking_fs
            .write_context(&parsed, "", "", true, &ctx.owner)?;
        self.viking_fs.write_content_raw(&parsed, content)?;

        // Enqueue for L0 generation + embedding
        if let Some(queue) = &self.embedding_queue {
            queue
                .enqueue(EmbeddingTask {
                    uri: uri.to_string(),
                    level: 0,
                    text: content.to_string(),
                })
                .await?;
        }

        Ok(())
    }

    /// Access the underlying VikingFs for direct operations.
    pub fn viking_fs(&self) -> &VikingFs {
        &self.viking_fs
    }
}

#[derive(Debug)]
pub struct StatResult {
    pub uri: String,
    pub is_leaf: bool,
    pub context_type: String,
    pub abstract_text: String,
    pub child_count: i64,
}
