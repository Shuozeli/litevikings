use async_trait::async_trait;
use lv_core::CoreError;

use super::config::ChatConfig;

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
}

#[derive(Debug)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
}

#[derive(Debug)]
pub struct ChatResponse {
    pub content: String,
}

/// Trait for chat completion.
#[async_trait]
pub trait ChatModel: Send + Sync {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, CoreError>;
}

/// Create a ChatModel from config.
pub fn create_chat(config: &ChatConfig) -> Box<dyn ChatModel> {
    match config {
        ChatConfig::OpenAi {
            base_url,
            model,
            api_key,
            temperature,
            max_tokens,
        } => Box::new(OpenAiChat::new(
            base_url,
            model,
            api_key.as_deref(),
            *temperature,
            *max_tokens,
        )),
        ChatConfig::Gemini {
            base_url,
            model,
            temperature,
            max_tokens,
        } => Box::new(GeminiChat::new(base_url, model, *temperature, *max_tokens)),
    }
}

// =============================================================================
// OpenAI-compatible chat client (vLLM, OpenAI, any compatible provider)
// =============================================================================

pub struct OpenAiChat {
    client: reqwest::Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
    #[allow(dead_code)]
    temperature: f32,
    max_tokens: Option<u32>,
}

impl OpenAiChat {
    pub fn new(
        base_url: &str,
        model: &str,
        api_key: Option<&str>,
        temperature: f32,
        max_tokens: Option<u32>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.map(String::from),
            temperature,
            max_tokens,
        }
    }
}

#[async_trait]
impl ChatModel for OpenAiChat {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, CoreError> {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "role": msg.role,
                    "content": msg.text,
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "temperature": request.temperature,
        });

        if let Some(max_tokens) = request.max_tokens.or(self.max_tokens) {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }

        let url = format!("{}/chat/completions", self.base_url);

        let mut req = self.client.post(&url).json(&body);
        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| CoreError::Internal(format!("openai chat request failed: {e}")))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Internal(format!("openai chat response parse failed: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(CoreError::Internal(format!(
                "openai chat API error ({}): {}",
                status, error_msg
            )));
        }

        let content = resp_body["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse { content })
    }
}

// =============================================================================
// Gemini-compatible chat client (internal proxy)
// =============================================================================

#[allow(dead_code)]
pub struct GeminiChat {
    client: reqwest::Client,
    base_url: String,
    model: String,
    temperature: f32,
    max_tokens: Option<u32>,
}

impl GeminiChat {
    pub fn new(base_url: &str, model: &str, temperature: f32, max_tokens: Option<u32>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            temperature,
            max_tokens,
        }
    }
}

#[async_trait]
impl ChatModel for GeminiChat {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse, CoreError> {
        let contents: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| {
                serde_json::json!({
                    "role": "user",
                    "parts": [{"text": msg.text}]
                })
            })
            .collect();

        let body = serde_json::json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature,
                "maxOutputTokens": request.max_tokens.or(self.max_tokens).unwrap_or(4096),
            }
        });

        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.base_url, self.model
        );

        let resp = self
            .client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| CoreError::Internal(format!("gemini request failed: {e}")))?;

        let status = resp.status();
        let resp_body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CoreError::Internal(format!("gemini response parse failed: {e}")))?;

        if !status.is_success() {
            let error_msg = resp_body["error"]["message"]
                .as_str()
                .unwrap_or("unknown error");
            return Err(CoreError::Internal(format!(
                "gemini API error ({}): {}",
                status, error_msg
            )));
        }

        let content = resp_body["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();

        Ok(ChatResponse { content })
    }
}
