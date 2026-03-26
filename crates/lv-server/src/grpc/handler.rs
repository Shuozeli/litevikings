use std::sync::Arc;

use tonic::{Request, Response, Status};

use lv_engine::storage::LsOptions;
use lv_engine::Engine;
use lv_proto::litevikings::v1::admin_service_server::AdminService;
use lv_proto::litevikings::v1::filesystem_service_server::FilesystemService;
use lv_proto::litevikings::v1::resources_service_server::ResourcesService;
use lv_proto::litevikings::v1::search_service_server::SearchService;
use lv_proto::litevikings::v1::sessions_service_server::SessionsService;
use lv_proto::litevikings::v1::*;

use super::auth::{extract_context, to_status};

/// gRPC service handler. Holds a reference to the Engine and delegates to services.
pub struct GrpcHandler {
    pub engine: Arc<Engine>,
}

impl Clone for GrpcHandler {
    fn clone(&self) -> Self {
        Self {
            engine: Arc::clone(&self.engine),
        }
    }
}

// --- FilesystemService ---

#[tonic::async_trait]
impl FilesystemService for GrpcHandler {
    async fn ls(&self, request: Request<LsRequest>) -> Result<Response<LsResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        let opts = LsOptions {
            simple: req.simple,
            recursive: req.recursive,
            node_limit: req.node_limit,
        };

        let entries = self
            .engine
            .fs
            .ls(&req.uri, &ctx, &opts)
            .map_err(to_status)?;

        Ok(Response::new(LsResponse {
            entries: entries
                .into_iter()
                .map(|e| DirEntry {
                    uri: e.uri,
                    is_leaf: e.is_leaf,
                    abstract_text: e.abstract_text,
                    context_type: e.context_type,
                    updated_at: e.updated_at,
                })
                .collect(),
        }))
    }

    async fn tree(&self, _request: Request<TreeRequest>) -> Result<Response<TreeResponse>, Status> {
        Err(Status::unimplemented("tree not yet implemented"))
    }

    async fn stat(&self, request: Request<StatRequest>) -> Result<Response<StatResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();

        let result = self.engine.fs.stat(&req.uri, &ctx).map_err(to_status)?;

        Ok(Response::new(StatResponse {
            uri: result.uri,
            is_leaf: result.is_leaf,
            context_type: result.context_type,
            abstract_text: result.abstract_text,
            child_count: result.child_count,
            created_at: String::new(),
            updated_at: String::new(),
        }))
    }

    async fn mkdir(
        &self,
        request: Request<MkdirRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        self.engine.fs.mkdir(&req.uri, &ctx).map_err(to_status)?;
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }

    async fn rm(&self, request: Request<RmRequest>) -> Result<Response<StatusResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        self.engine
            .fs
            .rm(&req.uri, &ctx, req.recursive)
            .map_err(to_status)?;
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }

    async fn mv(&self, request: Request<MvRequest>) -> Result<Response<StatusResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        self.engine
            .fs
            .mv(&req.from, &req.to, &ctx)
            .map_err(to_status)?;
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }

    async fn read(&self, request: Request<ReadRequest>) -> Result<Response<ReadResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        let content = self.engine.fs.read(&req.uri, &ctx).map_err(to_status)?;
        Ok(Response::new(ReadResponse { content }))
    }

    async fn read_abstract(
        &self,
        request: Request<ReadAbstractRequest>,
    ) -> Result<Response<ReadAbstractResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        let abstract_text = self
            .engine
            .fs
            .read_abstract(&req.uri, &ctx)
            .map_err(to_status)?;
        Ok(Response::new(ReadAbstractResponse { abstract_text }))
    }

    async fn read_overview(
        &self,
        request: Request<ReadOverviewRequest>,
    ) -> Result<Response<ReadOverviewResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        let overview = self
            .engine
            .fs
            .read_overview(&req.uri, &ctx)
            .map_err(to_status)?;
        Ok(Response::new(ReadOverviewResponse { overview }))
    }

    async fn write(
        &self,
        request: Request<WriteRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        self.engine
            .fs
            .write(&req.uri, &req.content, &ctx)
            .await
            .map_err(to_status)?;
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }
}

// --- SearchService ---

#[tonic::async_trait]
impl SearchService for GrpcHandler {
    async fn find(&self, request: Request<FindRequest>) -> Result<Response<FindResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();

        let result = self
            .engine
            .search
            .find(
                &req.query,
                req.target_uri.as_deref(),
                &ctx,
                req.limit as usize,
                req.score_threshold,
            )
            .await
            .map_err(to_status)?;

        Ok(Response::new(FindResponse {
            query: result.query,
            resources: result
                .resources
                .into_iter()
                .map(|m| MatchedContext {
                    uri: m.uri,
                    context_type: String::new(),
                    level: m.level,
                    abstract_text: m.abstract_text,
                    score: m.score as f32,
                    related: vec![],
                })
                .collect(),
            total_searched: result.total_searched as i64,
            rounds: result.rounds as i32,
        }))
    }

    async fn search(
        &self,
        request: Request<SearchWithSessionRequest>,
    ) -> Result<Response<FindResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();

        let result = self
            .engine
            .search
            .find(
                &req.query,
                req.target_uri.as_deref(),
                &ctx,
                req.limit as usize,
                req.score_threshold,
            )
            .await
            .map_err(to_status)?;

        Ok(Response::new(FindResponse {
            query: result.query,
            resources: result
                .resources
                .into_iter()
                .map(|m| MatchedContext {
                    uri: m.uri,
                    context_type: String::new(),
                    level: m.level,
                    abstract_text: m.abstract_text,
                    score: m.score as f32,
                    related: vec![],
                })
                .collect(),
            total_searched: result.total_searched as i64,
            rounds: result.rounds as i32,
        }))
    }

    async fn grep(&self, _request: Request<GrepRequest>) -> Result<Response<GrepResponse>, Status> {
        Err(Status::unimplemented("grep not yet implemented"))
    }

    async fn glob(&self, _request: Request<GlobRequest>) -> Result<Response<GlobResponse>, Status> {
        Err(Status::unimplemented("glob not yet implemented"))
    }
}

// --- ResourcesService ---

#[tonic::async_trait]
impl ResourcesService for GrpcHandler {
    async fn add_resource(
        &self,
        request: Request<AddResourceRequest>,
    ) -> Result<Response<AddResourceResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();

        let source = req
            .path
            .or(req.temp_path)
            .ok_or_else(|| Status::invalid_argument("either path or temp_path must be provided"))?;

        let result = self
            .engine
            .resources
            .add_resource(&source, req.to.as_deref(), req.wait, &ctx)
            .await
            .map_err(to_status)?;

        Ok(Response::new(AddResourceResponse {
            root_uri: result.root_uri,
            nodes_created: result.nodes_created as i64,
            processing_queued: result.processing_queued as i64,
        }))
    }

    async fn add_skill(
        &self,
        _request: Request<AddSkillRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        Err(Status::unimplemented("add_skill not yet implemented"))
    }

    async fn wait_processed(
        &self,
        _request: Request<WaitProcessedRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        self.engine.resources.wait_processed().await;
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }
}

// --- SessionsService ---

#[tonic::async_trait]
impl SessionsService for GrpcHandler {
    async fn create(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<CreateSessionResponse>, Status> {
        let ctx = extract_context(&request)?;
        let info = self.engine.sessions.create(&ctx).map_err(to_status)?;
        Ok(Response::new(CreateSessionResponse {
            session_id: info.session_id,
            session_uri: info.session_uri,
        }))
    }

    async fn get(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<GetSessionResponse>, Status> {
        let req = request.into_inner();
        let info = self
            .engine
            .sessions
            .get(&req.session_id)
            .map_err(to_status)?;
        Ok(Response::new(GetSessionResponse {
            session_id: info.session_id,
            session_uri: info.session_uri,
            owner_user: info.owner_user,
            compression: String::new(),
            stats: String::new(),
            created_at: String::new(),
        }))
    }

    async fn delete(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = request.into_inner();
        self.engine
            .sessions
            .delete(&req.session_id)
            .map_err(to_status)?;
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }

    async fn list(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let ctx = extract_context(&request)?;
        let sessions = self.engine.sessions.list(&ctx).map_err(to_status)?;
        Ok(Response::new(ListSessionsResponse {
            sessions: sessions
                .into_iter()
                .map(|s| GetSessionResponse {
                    session_id: s.session_id,
                    session_uri: s.session_uri,
                    owner_user: s.owner_user,
                    compression: String::new(),
                    stats: String::new(),
                    created_at: String::new(),
                })
                .collect(),
        }))
    }

    async fn add_message(
        &self,
        request: Request<AddMessageRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        let req = request.into_inner();
        let parts_json = if !req.parts.is_empty() {
            // Serialize parts to JSON
            let parts: Vec<serde_json::Value> = req.parts.iter().map(|p| {
                match &p.part {
                    Some(message_part::Part::Text(t)) => serde_json::json!({"type": "text", "text": t.text}),
                    Some(message_part::Part::Context(c)) => serde_json::json!({"type": "context", "uri": c.uri, "context_type": c.context_type, "abstract": c.abstract_text}),
                    Some(message_part::Part::Tool(t)) => serde_json::json!({"type": "tool", "tool_name": t.tool_name, "tool_output": t.tool_output}),
                    None => serde_json::json!({}),
                }
            }).collect();
            Some(serde_json::to_string(&parts).unwrap_or_default())
        } else {
            None
        };

        self.engine
            .sessions
            .add_message(
                &req.session_id,
                &req.role,
                req.content.as_deref(),
                parts_json.as_deref(),
            )
            .map_err(to_status)?;

        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }

    async fn get_messages(
        &self,
        request: Request<GetMessagesRequest>,
    ) -> Result<Response<GetMessagesResponse>, Status> {
        let req = request.into_inner();
        let messages = self
            .engine
            .sessions
            .get_messages(&req.session_id)
            .map_err(to_status)?;
        Ok(Response::new(GetMessagesResponse {
            messages: messages
                .into_iter()
                .map(|m| Message {
                    role: m.role,
                    parts: vec![], // TODO: parse parts JSON back to proto
                    timestamp: m.timestamp,
                })
                .collect(),
        }))
    }

    async fn commit(
        &self,
        request: Request<CommitRequest>,
    ) -> Result<Response<CommitResponse>, Status> {
        let ctx = extract_context(&request)?;
        let req = request.into_inner();
        let result = self
            .engine
            .sessions
            .commit(&req.session_id, &ctx)
            .await
            .map_err(to_status)?;
        Ok(Response::new(CommitResponse {
            memories_extracted: result.memories_extracted,
        }))
    }

    async fn record_usage(
        &self,
        _request: Request<RecordUsageRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        // TODO: implement usage recording
        Ok(Response::new(StatusResponse {
            status: "ok".to_string(),
            error: None,
        }))
    }
}

// --- AdminService ---

#[tonic::async_trait]
impl AdminService for GrpcHandler {
    async fn initialize(
        &self,
        _request: Request<InitializeRequest>,
    ) -> Result<Response<InitializeResponse>, Status> {
        Ok(Response::new(InitializeResponse {
            directories_created: 0,
        }))
    }

    async fn status(
        &self,
        _request: Request<StatusRequest>,
    ) -> Result<Response<SystemStatusResponse>, Status> {
        let status = self.engine.debug.status().map_err(to_status)?;
        Ok(Response::new(SystemStatusResponse {
            context_count: status.context_count,
            session_count: status.session_count,
            vector_count: status.vector_count,
            db_size_bytes: status.db_size_bytes,
        }))
    }
}
