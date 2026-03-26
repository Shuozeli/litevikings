use lv_core::CoreError;

use super::context::RequestContext;
use crate::parse::{ImportPipeline, ImportRequest, ImportResult};

/// Resource import service. Maps to upstream ResourceService.
pub struct ResourceService {
    pipeline: ImportPipeline,
}

impl ResourceService {
    pub fn new(pipeline: ImportPipeline) -> Self {
        Self { pipeline }
    }

    /// Import a file or URL as a resource.
    pub async fn add_resource(
        &self,
        source: &str,
        target_uri: Option<&str>,
        wait: bool,
        ctx: &RequestContext,
    ) -> Result<ImportResult, CoreError> {
        let req = ImportRequest {
            source: source.to_string(),
            target_uri: target_uri.map(String::from),
            wait,
        };
        self.pipeline.import(&req, ctx).await
    }

    /// Wait for all pending processing to complete.
    pub async fn wait_processed(&self) {
        self.pipeline.embedding_queue().flush().await;
    }
}
