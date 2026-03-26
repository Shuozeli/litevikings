use lv_core::Owner;
use lv_engine::service::RequestContext;
use tonic::{Request, Status};

use lv_engine::service::context::Role;

/// Extract RequestContext from gRPC metadata headers.
#[allow(clippy::result_large_err)]
pub fn extract_context<T>(request: &Request<T>) -> Result<RequestContext, Status> {
    let meta = request.metadata();

    let user_id = meta
        .get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default")
        .to_string();

    let account_id = meta
        .get("x-account-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default")
        .to_string();

    let agent_name = meta
        .get("x-agent-name")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    Ok(RequestContext {
        owner: Owner {
            account_id,
            user_id,
            agent_name,
        },
        role: Role::Root,
    })
}

/// Convert CoreError to gRPC Status.
pub fn to_status(err: lv_core::CoreError) -> Status {
    match err {
        lv_core::CoreError::NotFound(msg) => Status::not_found(msg),
        lv_core::CoreError::AlreadyExists(msg) => Status::already_exists(msg),
        lv_core::CoreError::InvalidArgument(msg) => Status::invalid_argument(msg),
        lv_core::CoreError::PermissionDenied(msg) => Status::permission_denied(msg),
        lv_core::CoreError::InvalidUri(msg) => Status::invalid_argument(msg),
        lv_core::CoreError::Internal(msg) => Status::internal(msg),
    }
}
