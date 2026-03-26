use serde::{Deserialize, Serialize};

/// Identifies who owns a context node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Owner {
    pub account_id: String,
    pub user_id: String,
    pub agent_name: Option<String>,
}

impl Owner {
    pub fn user_space_name(&self) -> &str {
        &self.user_id
    }

    pub fn agent_space_name(&self) -> Option<String> {
        self.agent_name
            .as_ref()
            .map(|agent| format!("{}:{}", self.user_id, agent))
    }
}

impl Default for Owner {
    fn default() -> Self {
        Self {
            account_id: "default".to_string(),
            user_id: "default".to_string(),
            agent_name: None,
        }
    }
}
