use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
pub struct ServeCmd {
    /// gRPC listen address
    #[arg(long, env = "LV_GRPC_ADDR")]
    grpc_addr: Option<String>,

    /// HTTP gateway listen address (for Python SDK compat)
    #[arg(long, env = "LV_HTTP_ADDR")]
    http_addr: Option<String>,

    /// LLM API base URL (Ollama, vLLM, or any OpenAI-compatible endpoint)
    #[arg(long, env = "LV_LLM_BASE_URL")]
    llm_base_url: Option<String>,

    /// Chat model name
    #[arg(long, env = "LV_CHAT_MODEL")]
    chat_model: Option<String>,

    /// Embedding model name
    #[arg(long, env = "LV_EMBED_MODEL")]
    embed_model: Option<String>,

    /// Embedding dimension
    #[arg(long, env = "LV_EMBED_DIM")]
    embed_dim: Option<usize>,

    /// Use local ONNX embeddings (no API needed for embeddings)
    #[arg(long)]
    local_embeddings: bool,
}

impl ServeCmd {
    pub async fn run(&self, data_dir: &str) -> Result<()> {
        let data_dir = shellexpand::tilde(data_dir).to_string();
        let data_path = std::path::PathBuf::from(&data_dir).join("data");

        let llm_base_url = self.llm_base_url.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "LLM base URL required. Set --llm-base-url or LV_LLM_BASE_URL env var.\n\
                 Example: lv serve --llm-base-url http://localhost:11434/v1\n\
                 Run `lv setup` to install and configure Ollama."
            )
        })?;

        let embedder_config = if self.local_embeddings {
            lv_engine::llm::EmbedderConfig::Local { model: None }
        } else {
            let embed_model = self.embed_model.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "Embedding model required. Set --embed-model or LV_EMBED_MODEL env var.\n\
                     Example: lv serve --embed-model nomic-embed-text\n\
                     Or use --local-embeddings for offline ONNX embeddings."
                )
            })?;
            let embed_dim = self.embed_dim.ok_or_else(|| {
                anyhow::anyhow!(
                    "Embedding dimension required. Set --embed-dim or LV_EMBED_DIM env var.\n\
                     Example: lv serve --embed-dim 768"
                )
            })?;
            lv_engine::llm::EmbedderConfig::OpenAi {
                base_url: llm_base_url.clone(),
                model: embed_model,
                api_key: std::env::var("LV_API_KEY").ok(),
                dimension: embed_dim,
            }
        };

        let chat_model = self.chat_model.clone().ok_or_else(|| {
            anyhow::anyhow!(
                "Chat model required. Set --chat-model or LV_CHAT_MODEL env var.\n\
                 Example: lv serve --chat-model qwen2.5:3b"
            )
        })?;

        let chat_config = lv_engine::llm::ChatConfig::OpenAi {
            base_url: llm_base_url,
            model: chat_model,
            api_key: std::env::var("LV_API_KEY").ok(),
            temperature: 0.3,
            max_tokens: None,
        };

        let config = lv_server::ServerConfig {
            grpc_addr: self
                .grpc_addr
                .clone()
                .unwrap_or_else(|| "0.0.0.0:50051".to_string()),
            http_addr: self
                .http_addr
                .clone()
                .unwrap_or_else(|| "0.0.0.0:1933".to_string()),
            engine: lv_engine::EngineConfig {
                storage: lv_engine::StorageConfig {
                    data_dir: data_path,
                },
                llm: lv_engine::llm::LlmConfig {
                    embedder: embedder_config,
                    chat: chat_config,
                },
            },
        };

        lv_server::serve(config)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }
}
