use serde::{Deserialize, Serialize};

use crate::error::CoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Session,
    User,
    Agent,
    Resources,
}

/// Parsed Viking URI with validated components.
///
/// Format: `viking://{scope}/{space}/{path...}`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VikingUri {
    raw: String,
    scope: Scope,
    /// Segments after `viking://{scope}/` (includes space + path)
    segments: Vec<String>,
}

impl VikingUri {
    pub fn parse(raw: &str) -> Result<Self, CoreError> {
        let stripped = raw
            .strip_prefix("viking://")
            .ok_or_else(|| CoreError::InvalidUri(format!("missing viking:// prefix: {raw}")))?;

        let parts: Vec<&str> = stripped.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Err(CoreError::InvalidUri(format!(
                "missing scope in URI: {raw}"
            )));
        }

        let scope = match parts[0] {
            "session" => Scope::Session,
            "user" => Scope::User,
            "agent" => Scope::Agent,
            "resources" => Scope::Resources,
            other => {
                return Err(CoreError::InvalidUri(format!(
                    "unknown scope '{other}' in URI: {raw}"
                )))
            }
        };

        let segments = parts[1..].iter().map(|s| s.to_string()).collect();

        Ok(Self {
            raw: raw.to_string(),
            scope,
            segments,
        })
    }

    pub fn as_str(&self) -> &str {
        &self.raw
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    pub fn depth(&self) -> usize {
        self.segments.len()
    }

    pub fn parent(&self) -> Option<VikingUri> {
        if self.segments.is_empty() {
            return None;
        }
        let parent_segments = &self.segments[..self.segments.len() - 1];
        let scope_str = scope_to_str(self.scope);
        let raw = if parent_segments.is_empty() {
            format!("viking://{scope_str}")
        } else {
            format!("viking://{scope_str}/{}", parent_segments.join("/"))
        };
        Some(VikingUri {
            raw,
            scope: self.scope,
            segments: parent_segments.to_vec(),
        })
    }

    pub fn child(&self, segment: &str) -> VikingUri {
        let raw = format!("{}/{segment}", self.raw.trim_end_matches('/'));
        let mut segments = self.segments.clone();
        segments.push(segment.to_string());
        VikingUri {
            raw,
            scope: self.scope,
            segments,
        }
    }

    pub fn is_ancestor_of(&self, other: &VikingUri) -> bool {
        if self.scope != other.scope {
            return false;
        }
        if self.segments.len() >= other.segments.len() {
            return false;
        }
        other.segments.starts_with(&self.segments)
    }

    /// Storage key for L0 abstract: `{uri}/.abstract.md`
    pub fn abstract_key(&self) -> String {
        format!("{}/.abstract.md", self.raw)
    }

    /// Storage key for L1 overview: `{uri}/.overview.md`
    pub fn overview_key(&self) -> String {
        format!("{}/.overview.md", self.raw)
    }

    /// Storage key for L2 content: `{uri}/.content`
    pub fn content_key(&self) -> String {
        format!("{}/.content", self.raw)
    }

    /// Storage key for relations: `{uri}/.relations.json`
    pub fn relations_key(&self) -> String {
        format!("{}/.relations.json", self.raw)
    }

    pub fn derive_context_type(&self) -> crate::ContextType {
        let s = self.raw.as_str();
        if s.contains("/skills") {
            crate::ContextType::Skill
        } else if s.contains("/memories") || self.scope == Scope::Session {
            crate::ContextType::Memory
        } else {
            crate::ContextType::Resource
        }
    }

    pub fn derive_category(&self) -> crate::Category {
        let s = self.raw.as_str();
        if s.contains("/preferences") {
            crate::Category::Preferences
        } else if s.contains("/entities") {
            crate::Category::Entities
        } else if s.contains("/events") {
            crate::Category::Events
        } else if s.contains("/cases") {
            crate::Category::Cases
        } else if s.contains("/patterns") {
            crate::Category::Patterns
        } else if s.contains("/profile") {
            crate::Category::Profile
        } else {
            crate::Category::None
        }
    }
}

impl std::fmt::Display for VikingUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.raw)
    }
}

fn scope_to_str(scope: Scope) -> &'static str {
    match scope {
        Scope::Session => "session",
        Scope::User => "user",
        Scope::Agent => "agent",
        Scope::Resources => "resources",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_uri() {
        let uri = VikingUri::parse("viking://user/alice/memories/preferences/code-style").unwrap();
        assert_eq!(uri.scope(), Scope::User);
        assert_eq!(
            uri.segments(),
            &["alice", "memories", "preferences", "code-style"]
        );
        assert_eq!(uri.depth(), 4);
    }

    #[test]
    fn parse_scope_only() {
        let uri = VikingUri::parse("viking://resources").unwrap();
        assert_eq!(uri.scope(), Scope::Resources);
        assert!(uri.segments().is_empty());
    }

    #[test]
    fn parse_invalid_prefix() {
        assert!(VikingUri::parse("http://user/foo").is_err());
    }

    #[test]
    fn parse_unknown_scope() {
        assert!(VikingUri::parse("viking://unknown/foo").is_err());
    }

    #[test]
    fn parent_and_child() {
        let uri = VikingUri::parse("viking://user/alice/memories").unwrap();
        let parent = uri.parent().unwrap();
        assert_eq!(parent.as_str(), "viking://user/alice");

        let child = uri.child("preferences");
        assert_eq!(child.as_str(), "viking://user/alice/memories/preferences");
    }

    #[test]
    fn parent_of_scope_root() {
        let uri = VikingUri::parse("viking://user").unwrap();
        assert!(uri.parent().is_none());
    }

    #[test]
    fn ancestor_check() {
        let parent = VikingUri::parse("viking://user/alice").unwrap();
        let child =
            VikingUri::parse("viking://user/alice/memories/preferences/code-style").unwrap();
        assert!(parent.is_ancestor_of(&child));
        assert!(!child.is_ancestor_of(&parent));
    }

    #[test]
    fn derive_context_type() {
        let mem = VikingUri::parse("viking://user/alice/memories/entities/foo").unwrap();
        assert_eq!(mem.derive_context_type(), crate::ContextType::Memory);

        let skill = VikingUri::parse("viking://agent/bot/skills/search").unwrap();
        assert_eq!(skill.derive_context_type(), crate::ContextType::Skill);

        let res = VikingUri::parse("viking://resources/docs/paper").unwrap();
        assert_eq!(res.derive_context_type(), crate::ContextType::Resource);
    }

    #[test]
    fn derive_category() {
        let pref = VikingUri::parse("viking://user/a/memories/preferences/style").unwrap();
        assert_eq!(pref.derive_category(), crate::Category::Preferences);

        let res = VikingUri::parse("viking://resources/docs").unwrap();
        assert_eq!(res.derive_category(), crate::Category::None);
    }

    #[test]
    fn storage_keys() {
        let uri = VikingUri::parse("viking://resources/docs/paper").unwrap();
        assert_eq!(
            uri.abstract_key(),
            "viking://resources/docs/paper/.abstract.md"
        );
        assert_eq!(
            uri.overview_key(),
            "viking://resources/docs/paper/.overview.md"
        );
        assert_eq!(uri.content_key(), "viking://resources/docs/paper/.content");
    }
}
