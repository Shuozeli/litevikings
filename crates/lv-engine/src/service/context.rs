use lv_core::Owner;

/// Request context extracted from gRPC metadata or HTTP headers.
#[derive(Debug, Clone)]
pub struct RequestContext {
    pub owner: Owner,
    pub role: Role,
}

#[derive(Debug, Clone, Copy)]
pub enum Role {
    Root,
    User,
    Agent,
}

impl RequestContext {
    /// Default context for local CLI usage.
    pub fn default_local() -> Self {
        Self {
            owner: Owner::default(),
            role: Role::Root,
        }
    }
}
