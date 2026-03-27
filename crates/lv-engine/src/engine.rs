use std::path::PathBuf;
use std::sync::Arc;

use lv_core::CoreError;
use serde::Deserialize;

use crate::llm::{create_chat, create_embedder, ChatModel, Embedder, LlmConfig};
use crate::parse::ImportPipeline;
use crate::service::{DebugService, FSService, ResourceService, SearchService, SessionService};
use crate::storage::{Database, EmbeddingQueue};

/// Top-level configuration for the engine.
#[derive(Debug, Deserialize, Clone)]
pub struct EngineConfig {
    pub storage: StorageConfig,
    pub llm: LlmConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    pub data_dir: PathBuf,
}

/// The Engine owns all subsystems. Only accessed through gRPC server.
pub struct Engine {
    pub db: Arc<Database>,
    pub embedder: Arc<dyn Embedder>,
    pub chat: Arc<dyn ChatModel>,
    pub embedding_queue: EmbeddingQueue,

    // Services
    pub fs: FSService,
    pub search: SearchService,
    pub resources: ResourceService,
    pub sessions: SessionService,
    pub debug: DebugService,

    _worker_handle: tokio::task::JoinHandle<()>,
}

impl Engine {
    pub async fn new(config: &EngineConfig) -> Result<Self, CoreError> {
        std::fs::create_dir_all(&config.storage.data_dir).map_err(|e| {
            CoreError::Internal(format!(
                "failed to create data dir {:?}: {e}",
                config.storage.data_dir
            ))
        })?;

        let db_path = config.storage.data_dir.join("litevikings.duckdb");
        let db = Arc::new(Database::open(&db_path)?);

        let embedder: Arc<dyn Embedder> = Arc::from(create_embedder(&config.llm.embedder).await?);
        let chat: Arc<dyn ChatModel> = Arc::from(create_chat(&config.llm.chat));

        // NOTE: HNSW index (vss extension) removed -- it downloads 50MB on first run
        // and blocks startup. Brute-force list_cosine_similarity() is fast enough
        // for <100K vectors (~200ms). Re-add when scale demands it.

        let worker_conn = db.clone_connection()?;
        let (embedding_queue, worker_handle) =
            EmbeddingQueue::spawn(worker_conn, Arc::clone(&embedder), Arc::clone(&chat));

        // Build services
        let fs = FSService::new(Arc::clone(&db), Some(embedding_queue.clone()));
        let search = SearchService::new(Arc::clone(&db), Arc::clone(&embedder));
        let debug = DebugService::new(Arc::clone(&db));
        let pipeline = ImportPipeline::new(Arc::clone(&db), embedding_queue.clone());
        let resources = ResourceService::new(pipeline);
        let sessions =
            SessionService::new(Arc::clone(&db), Arc::clone(&chat), embedding_queue.clone());

        tracing::info!(db_path = %db_path.display(), "engine initialized");

        Ok(Self {
            db,
            embedder,
            chat,
            embedding_queue,
            fs,
            search,
            resources,
            sessions,
            debug,
            _worker_handle: worker_handle,
        })
    }

    pub async fn shutdown(&self) {
        tracing::info!("engine shutting down, flushing embedding queue...");
        self.embedding_queue.flush().await;
        tracing::info!("engine shutdown complete");
    }
}
