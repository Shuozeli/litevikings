use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::context::ContextType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolStatus {
    Pending,
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Part {
    #[serde(rename = "text")]
    Text { text: String },

    #[serde(rename = "context")]
    Context {
        uri: String,
        context_type: ContextType,
        #[serde(rename = "abstract")]
        abstract_text: String,
    },

    #[serde(rename = "tool")]
    Tool {
        tool_id: String,
        tool_name: String,
        tool_uri: String,
        skill_uri: String,
        tool_input: Option<serde_json::Value>,
        tool_output: String,
        tool_status: ToolStatus,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub parts: Vec<Part>,
    pub timestamp: DateTime<Utc>,
}

impl Message {
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            parts: vec![Part::Text { text: text.into() }],
            timestamp: Utc::now(),
        }
    }
}
