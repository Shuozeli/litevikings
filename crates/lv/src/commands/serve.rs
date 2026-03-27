use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
pub struct ServeCmd {
    /// gRPC listen address
    #[arg(long, default_value = "0.0.0.0:50051")]
    grpc_addr: String,

    /// HTTP gateway listen address (for Python SDK compat)
    #[arg(long, default_value = "0.0.0.0:1933")]
    http_addr: String,

    /// LLM API base URL (Ollama, vLLM, or any OpenAI-compatible endpoint)
    #[arg(
        long,
        env = "LV_LLM_BASE_URL",
        default_value = "http://localhost:11434/v1"
    )]
    llm_base_url: String,

    /// Chat model name
    #[arg(long, env = "LV_CHAT_MODEL", default_value = "qwen2.5:3b")]
    chat_model: String,

    /// Embedding model name
    #[arg(long, env = "LV_EMBED_MODEL", default_value = "nomic-embed-text-v1.5")]
    embed_model: String,

    /// Embedding dimension
    #[arg(long, env = "LV_EMBED_DIM", default_value = "768")]
    embed_dim: usize,

    /// Use local ONNX embeddings (no vLLM needed for embeddings)
    #[arg(long)]
    local_embeddings: bool,
}

impl ServeCmd {
    pub async fn run(&self, data_dir: &str) -> Result<()> {
        let data_dir = shellexpand::tilde(data_dir).to_string();
        let data_path = std::path::PathBuf::from(&data_dir).join("data");

        let embedder_config = if self.local_embeddings {
            lv_engine::llm::EmbedderConfig::Local { model: None }
        } else {
            lv_engine::llm::EmbedderConfig::OpenAi {
                base_url: self.llm_base_url.clone(),
                model: self.embed_model.clone(),
                api_key: std::env::var("LV_API_KEY").ok(),
                dimension: self.embed_dim,
            }
        };

        let chat_config = lv_engine::llm::ChatConfig::OpenAi {
            base_url: self.llm_base_url.clone(),
            model: self.chat_model.clone(),
            api_key: std::env::var("LV_API_KEY").ok(),
            temperature: 0.3,
            max_tokens: None,
        };

        let config = lv_server::ServerConfig {
            grpc_addr: self.grpc_addr.clone(),
            http_addr: self.http_addr.clone(),
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
