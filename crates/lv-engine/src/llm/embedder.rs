use async_trait::async_trait;
use lv_core::CoreError;

use super::config::EmbedderConfig;

/// Trait for text embedding.
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError>;
    fn dimension(&self) -> usize;
}

/// Create an embedder from config.
pub async fn create_embedder(config: &EmbedderConfig) -> Result<Box<dyn Embedder>, CoreError> {
    match config {
        EmbedderConfig::OpenAi {
            base_url,
            model,
            api_key,
            dimension,
        } => Ok(Box::new(
            OpenAiEmbedder::new(base_url, model, api_key.as_deref(), *dimension).await?,
        )),
        EmbedderConfig::Local { model } => Ok(Box::new(LocalEmbedder::new(model.as_deref())?)),
    }
}

// =============================================================================
// OpenAI-compatible embedding client (vLLM, OpenAI, any compatible provider)
// =============================================================================

pub struct OpenAiEmbedder {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    dimension: usize,
}

impl OpenAiEmbedder {
    pub async fn new(
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
        dimension: usize,
    ) -> Result<Self, CoreError> {
        Ok(Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.map(String::from),
            dimension,
        })
    }
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        let body = serde_json::json!({
            "model": self.model,
            "input": text,
        });

        let url = format!("{}/embeddings", self.base_url);
        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CoreError::Internal(format!("embedding request failed: {e}")))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Internal(format!("embedding response parse failed: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(CoreError::Internal(format!(
                "embedding API error ({}): {}",
                status, error_msg
            )));
        }

        resp_body["data"][0]["embedding"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect()
            })
            .ok_or_else(|| CoreError::Internal("invalid embedding response".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        let url = format!("{}/embeddings", self.base_url);
        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CoreError::Internal(format!("batch embedding request failed: {e}")))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp.json().await.map_err(|e| {
            CoreError::Internal(format!("batch embedding response parse failed: {e}"))
        })?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(CoreError::Internal(format!(
                "batch embedding API error ({}): {}",
                status, error_msg
            )));
        }

        resp_body["data"]
            .as_array()
            .map(|data| {
                data.iter()
                    .map(|item| {
                        item["embedding"]
                            .as_array()
                            .unwrap_or(&vec![])
                            .iter()
                            .filter_map(|v| v.as_f64().map(|f| f as f32))
                            .collect()
                    })
                    .collect()
            })
            .ok_or_else(|| CoreError::Internal("invalid batch embedding response".to_string()))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}

// =============================================================================
// Local embedding via fastembed (ONNX runtime, offline fallback)
// =============================================================================

pub struct LocalEmbedder {
    model: fastembed::TextEmbedding,
    dimension: usize,
}

impl LocalEmbedder {
    pub fn new(model_name: Option<&str>) -> Result<Self, CoreError> {
        let model_info = match model_name {
            Some(name) => fastembed::EmbeddingModel::try_from(name.to_string()).map_err(|e| {
                CoreError::InvalidArgument(format!("unknown embedding model '{name}': {e}"))
            })?,
            None => fastembed::EmbeddingModel::BGESmallENV15,
        };

        let model = fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(model_info).with_show_download_progress(true),
        )
        .map_err(|e| CoreError::Internal(format!("failed to load embedding model: {e}")))?;

        let test = model
            .embed(vec!["test"], None)
            .map_err(|e| CoreError::Internal(format!("embedding test failed: {e}")))?;
        let dimension = test.first().map(|v| v.len()).unwrap_or(384);

        Ok(Self { model, dimension })
    }
}

#[async_trait]
impl Embedder for LocalEmbedder {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, CoreError> {
        let texts = vec![text.to_string()];
        let results = self
            .model
            .embed(texts, None)
            .map_err(|e| CoreError::Internal(format!("embedding failed: {e}")))?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| CoreError::Internal("empty embedding result".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, CoreError> {
        self.model
            .embed(texts.to_vec(), None)
            .map_err(|e| CoreError::Internal(format!("batch embedding failed: {e}")))
    }

    fn dimension(&self) -> usize {
        self.dimension
    }
}
