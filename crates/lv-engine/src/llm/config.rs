use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct LlmConfig {
    pub embedder: EmbedderConfig,
    pub chat: ChatConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "provider")]
pub enum EmbedderConfig {
    /// OpenAI-compatible embedding API (vLLM, OpenAI, etc.)
    /// This is the primary/default provider.
    #[serde(rename = "openai")]
    OpenAi {
        base_url: String,
        model: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default = "default_dimension")]
        dimension: usize,
    },
    /// Local ONNX embedding via fastembed (offline fallback, no API calls).
    #[serde(rename = "local")]
    Local { model: Option<String> },
}

impl Default for EmbedderConfig {
    fn default() -> Self {
        Self::OpenAi {
            base_url: "http://localhost:8000/v1".to_string(),
            model: "nomic-embed-text-v1.5".to_string(),
            api_key: None,
            dimension: 768,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "provider")]
pub enum ChatConfig {
    /// OpenAI-compatible chat API (vLLM, OpenAI, etc.)
    /// This is the primary/default provider.
    #[serde(rename = "openai")]
    OpenAi {
        base_url: String,
        model: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default = "default_temperature")]
        temperature: f32,
        #[serde(default)]
        max_tokens: Option<u32>,
    },
    /// Gemini-compatible API (internal proxy).
    #[serde(rename = "gemini")]
    Gemini {
        base_url: String,
        model: String,
        #[serde(default = "default_temperature")]
        temperature: f32,
        #[serde(default)]
        max_tokens: Option<u32>,
    },
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self::OpenAi {
            base_url: "http://localhost:8000/v1".to_string(),
            model: "gemma-3-4b-it".to_string(),
            api_key: None,
            temperature: 0.3,
            max_tokens: None,
        }
    }
}

fn default_temperature() -> f32 {
    0.3
}

fn default_dimension() -> usize {
    768
}
