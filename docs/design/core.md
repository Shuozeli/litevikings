# lv-core: Data Models & URI Scheme

## Overview

`lv-core` is the foundational crate with zero internal dependencies. It defines the
data types, URI scheme, error types, and preset directory structure used by every other
crate in the workspace.

## Viking URI Scheme

All content in OpenViking is addressed by a `viking://` URI:

```
viking://{scope}/{space}/{path...}
```

### Scopes

| Scope | Purpose | Space key |
|-------|---------|-----------|
| `session` | Temporary conversation data | `{user_space}/{session_id}` |
| `user` | Long-term user memories | `{user_space}` |
| `agent` | Agent learning, instructions, skills | `{agent_space}` |
| `resources` | Imported knowledge base | `{user_space}` or global |

### URI Examples

```
viking://user/alice/memories/preferences/code-style
viking://agent/alice:coding-agent/skills/web-search
viking://session/alice/abc-123/messages
viking://resources/alice/papers/transformers
```

### Rust Type

```rust
/// Parsed Viking URI with validated components.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VikingUri {
    /// Full URI string (e.g., "viking://user/alice/memories")
    raw: String,
    /// Scope: session, user, agent, resources
    scope: Scope,
    /// Space identifier (user or agent space name)
    space: Option<String>,
    /// Path segments after scope/space
    segments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Scope {
    Session,
    User,
    Agent,
    Resources,
}

impl VikingUri {
    pub fn parse(raw: &str) -> Result<Self, UriError>;
    pub fn parent(&self) -> Option<VikingUri>;
    pub fn child(&self, segment: &str) -> VikingUri;
    pub fn is_ancestor_of(&self, other: &VikingUri) -> bool;
    pub fn scope(&self) -> Scope;
    pub fn depth(&self) -> usize;

    /// Storage key for L0 abstract
    pub fn abstract_key(&self) -> String; // "{uri}/.abstract.md"
    /// Storage key for L1 overview
    pub fn overview_key(&self) -> String; // "{uri}/.overview.md"
    /// Storage key for content
    pub fn content_key(&self) -> String;  // "{uri}/.content"
    /// Storage key for relations
    pub fn relations_key(&self) -> String; // "{uri}/.relations.json"
}
```

## Context

The fundamental data unit. Every node in the Viking tree is a `Context`.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// Unique identifier (UUID v4)
    pub id: Uuid,
    /// Viking URI address
    pub uri: VikingUri,
    /// Parent URI (None for scope roots)
    pub parent_uri: Option<VikingUri>,
    /// Whether this is a leaf node (no children)
    pub is_leaf: bool,
    /// L0 abstract text (~100 tokens)
    pub abstract_text: String,
    /// Context type derived from URI
    pub context_type: ContextType,
    /// Category derived from URI path
    pub category: Category,
    /// Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Activity counter (incremented on each access)
    pub active_count: u64,
    /// Related URIs (cross-references)
    pub related_uri: Vec<VikingUri>,
    /// Arbitrary metadata
    pub meta: HashMap<String, serde_json::Value>,
    /// Context level (L0/L1/L2) -- used for vector index records
    pub level: Option<ContextLevel>,
    /// Session ID (for session-scoped contexts)
    pub session_id: Option<String>,
    /// Owner identity
    pub owner: Owner,
}
```

## Enums

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextType {
    Skill,
    Memory,
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContextLevel {
    /// L0: ~100 token abstract for vector search
    Abstract = 0,
    /// L1: ~1k token overview for navigation
    Overview = 1,
    /// L2: Full content, on-demand
    Detail = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Category {
    Preferences,
    Entities,
    Events,
    Cases,
    Patterns,
    Profile,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceContentType {
    Text,
    Image,
    Video,
    Audio,
    Binary,
}
```

## Owner Identity

```rust
/// Identifies who owns a context node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Owner {
    /// Account-level ID (tenant)
    pub account_id: String,
    /// User identifier
    pub user_id: String,
    /// Agent name (optional, for agent-scoped contexts)
    pub agent_name: Option<String>,
}

impl Owner {
    pub fn user_space_name(&self) -> String;
    pub fn agent_space_name(&self) -> Option<String>;
}
```

## Preset Directory Structure

Defined as a compile-time constant tree. Initialized on first use for each user/agent.

```rust
pub struct DirectoryPreset {
    /// Relative path under scope root
    pub path: &'static str,
    /// L0 abstract text
    pub abstract_text: &'static str,
    /// L1 overview text
    pub overview_text: &'static str,
    /// Child directories
    pub children: &'static [DirectoryPreset],
}

/// Returns the preset tree for a given scope.
pub fn preset_directories(scope: Scope) -> &'static DirectoryPreset;
```

### Preset Tree

```
user/
  memories/
    preferences/   -- "User preferences by topic"
    entities/      -- "Entity memories (projects, people, concepts)"
    events/        -- "Event records (decisions, milestones)"

agent/
  memories/
    cases/         -- "Specific problems and solutions"
    patterns/      -- "Reusable patterns and best practices"
  instructions/    -- "Behavioral directives and rules"
  skills/          -- "Callable skill definitions"

session/           -- "Temporary conversation data"

resources/         -- "Imported knowledge base"
```

## Relations

Cross-references between contexts.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub id: String,
    pub uris: Vec<VikingUri>,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}
```

## Error Types

```rust
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid URI: {0}")]
    InvalidUri(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("internal: {0}")]
    Internal(String),
}
```

## Derivation Rules

Context type and category are derived from the URI path, matching upstream behavior:

```rust
impl VikingUri {
    pub fn derive_context_type(&self) -> ContextType {
        let s = self.raw.as_str();
        if s.contains("/skills") {
            ContextType::Skill
        } else if s.contains("/memories") || self.scope == Scope::Session {
            ContextType::Memory
        } else {
            ContextType::Resource
        }
    }

    pub fn derive_category(&self) -> Category {
        let s = self.raw.as_str();
        if s.contains("/preferences") { Category::Preferences }
        else if s.contains("/entities") { Category::Entities }
        else if s.contains("/events") { Category::Events }
        else if s.contains("/cases") { Category::Cases }
        else if s.contains("/patterns") { Category::Patterns }
        else if s.contains("/profile") { Category::Profile }
        else { Category::None }
    }
}
```

## Crate Dependencies

```toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
```

Zero async runtime dependency. Pure data types + validation logic.
