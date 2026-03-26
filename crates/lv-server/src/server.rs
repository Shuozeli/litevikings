use std::sync::Arc;

use lv_engine::{Engine, EngineConfig};
use serde::Deserialize;
use tonic::transport::Server;

use lv_proto::litevikings::v1::admin_service_server::AdminServiceServer;
use lv_proto::litevikings::v1::filesystem_service_server::FilesystemServiceServer;
use lv_proto::litevikings::v1::resources_service_server::ResourcesServiceServer;
use lv_proto::litevikings::v1::search_service_server::SearchServiceServer;
use lv_proto::litevikings::v1::sessions_service_server::SessionsServiceServer;

use crate::grpc::GrpcHandler;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub grpc_addr: String,
    pub http_addr: String,
    pub engine: EngineConfig,
}

pub async fn serve(config: ServerConfig) -> Result<(), lv_core::CoreError> {
    let engine = Arc::new(Engine::new(&config.engine).await?);

    let handler = GrpcHandler {
        engine: Arc::clone(&engine),
    };

    let grpc_addr: std::net::SocketAddr = config
        .grpc_addr
        .parse()
        .map_err(|e| lv_core::CoreError::InvalidArgument(format!("invalid grpc_addr: {e}")))?;

    tracing::info!("gRPC server listening on {}", grpc_addr);

    Server::builder()
        .add_service(FilesystemServiceServer::new(handler.clone()))
        .add_service(SearchServiceServer::new(handler.clone()))
        .add_service(ResourcesServiceServer::new(handler.clone()))
        .add_service(SessionsServiceServer::new(handler.clone()))
        .add_service(AdminServiceServer::new(handler))
        .serve(grpc_addr)
        .await
        .map_err(|e| lv_core::CoreError::Internal(format!("gRPC server error: {e}")))?;

    Ok(())
}
