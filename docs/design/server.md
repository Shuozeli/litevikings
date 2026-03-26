# lv-server: gRPC Server + HTTP Gateway

## Overview

`lv-server` hosts two listeners in one process:

1. **gRPC server** (tonic) -- the primary protocol. CLI and internal clients connect here.
2. **HTTP gateway** (axum) -- translates REST/JSON to gRPC for upstream Python SDK compatibility.

Both call the same gRPC service implementations, which delegate to `lv-engine`.

## Architecture

```
lv-server/
  src/
    lib.rs
    server.rs              -- Start both gRPC + HTTP listeners
    config.rs              -- ServerConfig

    grpc/
      mod.rs
      filesystem.rs        -- impl FilesystemService for GrpcHandler
      resources.rs         -- impl ResourcesService for GrpcHandler
      sessions.rs          -- impl SessionsService for GrpcHandler
      search.rs            -- impl SearchService for GrpcHandler
      relations.rs         -- impl RelationsService for GrpcHandler
      admin.rs             -- impl AdminService for GrpcHandler
      auth.rs              -- tonic interceptor: extract RequestContext from metadata

    http/
      mod.rs
      gateway.rs           -- axum Router: REST routes that call gRPC client
      auth.rs              -- axum middleware: extract API key -> gRPC metadata
      error.rs             -- CoreError -> JSON response mapping
```

## gRPC Server

Each gRPC service implementation holds a reference to `Engine` and translates
between proto types and engine types.

```rust
pub struct GrpcHandler {
    engine: Arc<Engine>,
}

#[tonic::async_trait]
impl FilesystemService for GrpcHandler {
    async fn ls(
        &self,
        request: Request<LsRequest>,
    ) -> Result<Response<LsResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        let opts = LsOptions {
            simple: req.simple,
            recursive: req.recursive,
            output: req.output,
            abs_limit: req.abs_limit,
            show_all_hidden: req.show_all_hidden,
            node_limit: req.node_limit,
        };
        let result = self.engine.fs.ls(&req.uri, &ctx, &opts)
            .map_err(to_status)?;

        Ok(Response::new(LsResponse {
            entries: result.into_iter().map(to_proto_dir_entry).collect(),
        }))
    }

    // ... other methods follow the same pattern ...
}
```

### Auth Interceptor

Extracts `RequestContext` from gRPC metadata headers.

```rust
pub fn auth_interceptor(
    api_keys: Arc<Vec<String>>,
) -> impl Fn(Request<()>) -> Result<Request<()>, Status> {
    move |mut req: Request<()>| {
        let meta = req.metadata();

        // Trusted mode: no key required
        if api_keys.is_empty() {
            return Ok(req);
        }

        // API key mode
        let key = meta.get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| Status::unauthenticated("missing x-api-key"))?;

        if !api_keys.contains(&key.to_string()) {
            return Err(Status::unauthenticated("invalid api key"));
        }

        Ok(req)
    }
}

fn extract_context<T>(request: &Request<T>) -> Result<RequestContext, Status> {
    let meta = request.metadata();
    let user_id = meta.get("x-user-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default")
        .to_string();
    let account_id = meta.get("x-account-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default")
        .to_string();
    let agent_name = meta.get("x-agent-name")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    Ok(RequestContext {
        owner: Owner { account_id, user_id, agent_name },
        role: Role::Root,
    })
}
```

### Error Mapping (engine -> gRPC status)

```rust
fn to_status(err: CoreError) -> Status {
    match err {
        CoreError::NotFound(msg) => Status::not_found(msg),
        CoreError::AlreadyExists(msg) => Status::already_exists(msg),
        CoreError::InvalidArgument(msg) => Status::invalid_argument(msg),
        CoreError::PermissionDenied(msg) => Status::permission_denied(msg),
        CoreError::InvalidUri(msg) => Status::invalid_argument(msg),
        CoreError::Internal(msg) => Status::internal(msg),
    }
}
```

## HTTP Gateway

The HTTP gateway is an axum app where each REST endpoint creates a gRPC request,
calls the in-process gRPC client, and translates the response to JSON.

### Why in-process gRPC client?

The gateway runs in the same process as the gRPC server. It uses tonic's `Channel`
connected to the local gRPC endpoint. This means:
- Zero-copy where possible (tonic optimizes local channels)
- Same auth flow (HTTP middleware adds gRPC metadata)
- Single code path (gateway is just another gRPC client)

```rust
pub struct HttpGateway {
    fs_client: FilesystemServiceClient<Channel>,
    resources_client: ResourcesServiceClient<Channel>,
    sessions_client: SessionsServiceClient<Channel>,
    search_client: SearchServiceClient<Channel>,
    relations_client: RelationsServiceClient<Channel>,
    admin_client: AdminServiceClient<Channel>,
}

impl HttpGateway {
    pub async fn new(grpc_addr: &str) -> Result<Self> {
        let channel = Channel::from_shared(format!("http://{}", grpc_addr))?
            .connect()
            .await?;
        Ok(Self {
            fs_client: FilesystemServiceClient::new(channel.clone()),
            resources_client: ResourcesServiceClient::new(channel.clone()),
            sessions_client: SessionsServiceClient::new(channel.clone()),
            search_client: SearchServiceClient::new(channel.clone()),
            relations_client: RelationsServiceClient::new(channel.clone()),
            admin_client: AdminServiceClient::new(channel),
        })
    }
}
```

### HTTP Route Example

```rust
// GET /api/v1/fs/ls -> gRPC FilesystemService::Ls
async fn http_ls(
    State(gw): State<Arc<HttpGateway>>,
    Query(params): Query<LsParams>,
) -> impl IntoResponse {
    let mut client = gw.fs_client.clone();
    let request = tonic::Request::new(LsRequest {
        uri: params.uri,
        simple: params.simple.unwrap_or(false),
        recursive: params.recursive.unwrap_or(false),
        output: params.output.unwrap_or_else(|| "agent".into()),
        abs_limit: params.abs_limit.unwrap_or(256),
        show_all_hidden: params.show_all_hidden.unwrap_or(false),
        node_limit: params.node_limit.unwrap_or(1000),
    });
    // Forward API key from HTTP header to gRPC metadata
    inject_auth(&params.api_key, &mut request);

    match client.ls(request).await {
        Ok(resp) => {
            let result = resp.into_inner();
            Json(json!({ "status": "ok", "result": to_json_entries(&result.entries) }))
                .into_response()
        }
        Err(status) => status_to_http_error(status).into_response(),
    }
}
```

### HTTP Gateway Router

```rust
pub fn build_http_router(gateway: Arc<HttpGateway>) -> Router {
    Router::new()
        // Filesystem
        .route("/api/v1/fs/ls", get(http_ls))
        .route("/api/v1/fs/tree", get(http_tree))
        .route("/api/v1/fs/stat", get(http_stat))
        .route("/api/v1/fs/mkdir", post(http_mkdir))
        .route("/api/v1/fs/rm", post(http_rm))
        .route("/api/v1/fs/mv", post(http_mv))
        .route("/api/v1/fs/read", get(http_read))
        .route("/api/v1/fs/abstract", get(http_abstract))
        .route("/api/v1/fs/overview", get(http_overview))
        .route("/api/v1/fs/write", post(http_write))
        // Resources
        .route("/api/v1/add-resource", post(http_add_resource))
        .route("/api/v1/add-skill", post(http_add_skill))
        .route("/api/v1/wait-processed", get(http_wait_processed))
        // Sessions
        .route("/api/v1/sessions", post(http_create_session))
        .route("/api/v1/sessions", get(http_list_sessions))
        .route("/api/v1/sessions/:id", get(http_get_session))
        .route("/api/v1/sessions/:id", delete(http_delete_session))
        .route("/api/v1/sessions/:id/messages", post(http_add_message))
        .route("/api/v1/sessions/:id/messages", get(http_get_messages))
        .route("/api/v1/sessions/:id/commit", post(http_commit))
        .route("/api/v1/sessions/:id/used", post(http_record_usage))
        // Search
        .route("/api/v1/search/find", post(http_find))
        .route("/api/v1/search/search", post(http_search))
        .route("/api/v1/search/grep", post(http_grep))
        .route("/api/v1/search/glob", post(http_glob))
        // Relations
        .route("/api/v1/relations", get(http_get_relations))
        .route("/api/v1/relations/link", post(http_link))
        .route("/api/v1/relations/unlink", post(http_unlink))
        // Admin
        .route("/api/v1/admin/initialize", post(http_initialize))
        .route("/api/v1/admin/status", get(http_status))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(gateway)
}
```

### gRPC Status -> HTTP Error

```rust
fn status_to_http_error(status: Status) -> (StatusCode, Json<serde_json::Value>) {
    let (http_code, error_code) = match status.code() {
        Code::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        Code::AlreadyExists => (StatusCode::CONFLICT, "ALREADY_EXISTS"),
        Code::InvalidArgument => (StatusCode::BAD_REQUEST, "INVALID_ARGUMENT"),
        Code::PermissionDenied => (StatusCode::FORBIDDEN, "PERMISSION_DENIED"),
        Code::Unauthenticated => (StatusCode::UNAUTHORIZED, "UNAUTHENTICATED"),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL"),
    };
    (http_code, Json(json!({
        "status": "error",
        "error": { "code": error_code, "message": status.message() }
    })))
}
```

## Server Startup

```rust
pub async fn serve(config: ServerConfig) -> Result<()> {
    // 1. Initialize engine
    let engine = Arc::new(Engine::new(&config.engine).await?);

    // 2. Build gRPC server
    let handler = GrpcHandler { engine };
    let grpc_server = Server::builder()
        .add_service(FilesystemServiceServer::with_interceptor(
            handler.clone(), auth_interceptor(config.api_keys.clone())))
        .add_service(ResourcesServiceServer::with_interceptor(
            handler.clone(), auth_interceptor(config.api_keys.clone())))
        .add_service(SessionsServiceServer::with_interceptor(
            handler.clone(), auth_interceptor(config.api_keys.clone())))
        .add_service(SearchServiceServer::with_interceptor(
            handler.clone(), auth_interceptor(config.api_keys.clone())))
        .add_service(RelationsServiceServer::with_interceptor(
            handler.clone(), auth_interceptor(config.api_keys.clone())))
        .add_service(AdminServiceServer::with_interceptor(
            handler.clone(), auth_interceptor(config.api_keys.clone())));

    // 3. Start gRPC listener
    let grpc_addr = config.grpc_addr.parse()?;
    let grpc_handle = tokio::spawn(async move {
        tracing::info!("gRPC server listening on {}", grpc_addr);
        grpc_server.serve(grpc_addr).await
    });

    // 4. Build HTTP gateway (connects to local gRPC)
    let gateway = Arc::new(HttpGateway::new(&config.grpc_addr).await?);
    let http_app = build_http_router(gateway);

    // 5. Start HTTP listener
    let http_addr = config.http_addr.parse()?;
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!("HTTP gateway listening on {}", http_addr);
    let http_handle = tokio::spawn(async move {
        axum::serve(http_listener, http_app).await
    });

    // 6. Wait for either to finish
    tokio::select! {
        r = grpc_handle => r??,
        r = http_handle => r??,
    }
    Ok(())
}
```

## Configuration

```rust
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub grpc_addr: String,       // default: "0.0.0.0:50051"
    pub http_addr: String,       // default: "0.0.0.0:1933"
    pub api_keys: Arc<Vec<String>>,
    pub engine: EngineConfig,
}
```

## Crate Dependencies

```toml
[dependencies]
lv-core = { path = "../lv-core" }
lv-proto = { path = "../lv-proto" }
lv-engine = { path = "../lv-engine" }

tonic = { workspace = true }
prost = { workspace = true }
axum = { workspace = true }
tower-http = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

## TODO / Deferred

- **gRPC streaming**: WaitProcessed progress, real-time session events
- **gRPC reflection**: For grpcurl/grpcui debugging
- **Prometheus metrics**: Request latency, storage stats
- **Rate limiting**: Per API key
- **File upload**: Multipart form -> temp file -> AddResource (HTTP gateway only)
- **TLS**: Feature-flag rustls for gRPC + HTTP
