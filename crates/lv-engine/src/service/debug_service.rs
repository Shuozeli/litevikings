use std::sync::Arc;

use lv_core::CoreError;

use crate::storage::Database;

/// Debug/admin service for system status.
pub struct DebugService {
    db: Arc<Database>,
}

impl DebugService {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    pub fn status(&self) -> Result<SystemStatus, CoreError> {
        self.db.with_conn(|conn| {
            let context_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM contexts", [], |row| row.get(0))
                .map_err(|e| CoreError::Internal(format!("count contexts: {e}")))?;

            let session_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))
                .map_err(|e| CoreError::Internal(format!("count sessions: {e}")))?;

            let vector_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM contexts WHERE vector IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| CoreError::Internal(format!("count vectors: {e}")))?;

            let pending_embeddings = context_count - vector_count;

            Ok(SystemStatus {
                context_count,
                session_count,
                vector_count,
                pending_embeddings,
                db_size_bytes: 0,
            })
        })
    }
}

#[derive(Debug)]
pub struct SystemStatus {
    pub context_count: i64,
    pub session_count: i64,
    pub vector_count: i64,
    pub pending_embeddings: i64,
    pub db_size_bytes: i64,
}
