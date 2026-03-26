use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::owner::Owner;
use crate::uri::VikingUri;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextType {
    Skill,
    Memory,
    Resource,
}

impl ContextType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Memory => "memory",
            Self::Resource => "resource",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum ContextLevel {
    Abstract = 0,
    Overview = 1,
    Detail = 2,
}

impl ContextLevel {
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Self::Abstract),
            1 => Some(Self::Overview),
            2 => Some(Self::Detail),
            _ => None,
        }
    }

    pub fn as_i32(&self) -> i32 {
        *self as i32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Preferences,
    Entities,
    Events,
    Cases,
    Patterns,
    Profile,
    None,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preferences => "preferences",
            Self::Entities => "entities",
            Self::Events => "events",
            Self::Cases => "cases",
            Self::Patterns => "patterns",
            Self::Profile => "profile",
            Self::None => "",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResourceContentType {
    Text,
    Image,
    Video,
    Audio,
    Binary,
}

/// The fundamental data unit. Every node in the Viking tree is a Context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub id: Uuid,
    pub uri: VikingUri,
    pub parent_uri: Option<VikingUri>,
    pub is_leaf: bool,
    pub abstract_text: String,
    pub context_type: ContextType,
    pub category: Category,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub active_count: u64,
    pub related_uri: Vec<VikingUri>,
    pub meta: HashMap<String, serde_json::Value>,
    pub level: Option<ContextLevel>,
    pub session_id: Option<String>,
    pub owner: Owner,
    pub vector: Option<Vec<f32>>,
}

impl Context {
    pub fn new(uri: VikingUri, owner: Owner) -> Self {
        let context_type = uri.derive_context_type();
        let category = uri.derive_category();
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            uri,
            parent_uri: None,
            is_leaf: false,
            abstract_text: String::new(),
            context_type,
            category,
            created_at: now,
            updated_at: now,
            active_count: 0,
            related_uri: Vec::new(),
            meta: HashMap::new(),
            level: None,
            session_id: None,
            owner,
            vector: None,
        }
    }

    pub fn parent_uri_str(&self) -> Option<&str> {
        self.parent_uri.as_ref().map(|u| u.as_str())
    }

    pub fn level_i32(&self) -> Option<i32> {
        self.level.map(|l| l.as_i32())
    }

    pub fn update_activity(&mut self) {
        self.active_count += 1;
        self.updated_at = Utc::now();
    }
}
