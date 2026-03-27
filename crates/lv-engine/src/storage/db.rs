use std::path::{Path, PathBuf};
use std::sync::Mutex;

use duckdb::Connection;
use lv_core::CoreError;

use super::schema::SCHEMA_SQL;

/// DuckDB database handle. Thread-safe via Mutex.
///
/// DuckDB Connection contains RefCell (not Sync). We wrap it in a Mutex
/// so Arc<Database> can be shared across async tasks in the gRPC server.
///
/// Each operation locks the mutex, does its work, and releases.
/// For the embedding worker, a separate Connection is cloned before spawning.
pub struct Database {
    conn: Mutex<Connection>,
    #[allow(dead_code)]
    path: Option<PathBuf>,
}

// Safety: Connection is protected by Mutex
unsafe impl Sync for Database {}

impl Database {
    /// Open or create a database file at the given path.
    pub fn open(path: &Path) -> Result<Self, CoreError> {
        let conn = Connection::open(path)
            .map_err(|e| CoreError::Internal(format!("failed to open database: {e}")))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| CoreError::Internal(format!("schema migration failed: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: Some(path.to_path_buf()),
        })
    }

    /// Open an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| CoreError::Internal(format!("failed to open in-memory database: {e}")))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| CoreError::Internal(format!("schema migration failed: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: None,
        })
    }

    /// Get a clone of the underlying connection (for background workers).
    /// The cloned connection shares the same DuckDB instance.
    pub fn clone_connection(&self) -> Result<Connection, CoreError> {
        let guard = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("database mutex poisoned: {e}")))?;
        guard
            .try_clone()
            .map_err(|e| CoreError::Internal(format!("failed to clone connection: {e}")))
    }

    /// Execute a function with access to the connection.
    /// This is the primary way to interact with the database.
    pub fn with_conn<F, T>(&self, f: F) -> Result<T, CoreError>
    where
        F: FnOnce(&Connection) -> Result<T, CoreError>,
    {
        let guard = self
            .conn
            .lock()
            .map_err(|e| CoreError::Internal(format!("database mutex poisoned: {e}")))?;
        f(&guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory_and_create_schema() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM contexts", [], |row| row.get(0))
                .map_err(|e| CoreError::Internal(e.to_string()))?;
            assert_eq!(count, 0);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn insert_and_query_context() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO contexts (id, uri, level, is_leaf, context_type, owner_user, abstract_text)
                 VALUES ('ctx-1', 'viking://resources/docs', 0, false, 'resource', 'alice', 'Test abstract')",
                [],
            )
            .map_err(|e| CoreError::Internal(e.to_string()))?;

            let abstract_text: String = conn
                .query_row(
                    "SELECT abstract_text FROM contexts WHERE uri = 'viking://resources/docs' AND level = 0",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| CoreError::Internal(e.to_string()))?;
            assert_eq!(abstract_text, "Test abstract");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn insert_and_query_content() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO content (key, data) VALUES ('viking://resources/docs/.abstract.md', 'hello world')",
                [],
            )
            .map_err(|e| CoreError::Internal(e.to_string()))?;

            let data: Vec<u8> = conn
                .query_row(
                    "SELECT data FROM content WHERE key = 'viking://resources/docs/.abstract.md'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| CoreError::Internal(e.to_string()))?;
            assert_eq!(data, b"hello world");
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn vector_search() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO contexts (id, uri, level, is_leaf, context_type, owner_user, abstract_text, vector)
                 VALUES ('ctx-1', 'viking://resources/a', 0, true, 'resource', 'alice', 'Rust programming', [1.0, 0.0, 0.0])",
                [],
            ).map_err(|e| CoreError::Internal(e.to_string()))?;

            conn.execute(
                "INSERT INTO contexts (id, uri, level, is_leaf, context_type, owner_user, abstract_text, vector)
                 VALUES ('ctx-2', 'viking://resources/b', 0, true, 'resource', 'alice', 'Python programming', [0.0, 1.0, 0.0])",
                [],
            ).map_err(|e| CoreError::Internal(e.to_string()))?;

            conn.execute(
                "INSERT INTO contexts (id, uri, level, is_leaf, context_type, owner_user, abstract_text, vector)
                 VALUES ('ctx-3', 'viking://resources/c', 0, true, 'resource', 'alice', 'Rust async', [0.9, 0.1, 0.0])",
                [],
            ).map_err(|e| CoreError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT uri, abstract_text,
                            list_cosine_similarity(vector, [1.0, 0.0, 0.0]) AS score
                     FROM contexts
                     WHERE vector IS NOT NULL
                     ORDER BY score DESC
                     LIMIT 2",
                )
                .map_err(|e| CoreError::Internal(e.to_string()))?;

            let results: Vec<(String, String, f64)> = stmt
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .map_err(|e| CoreError::Internal(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CoreError::Internal(e.to_string()))?;

            assert_eq!(results.len(), 2);
            assert_eq!(results[0].0, "viking://resources/a");
            assert!(results[0].2 > 0.99);
            assert_eq!(results[1].0, "viking://resources/c");
            assert!(results[1].2 > 0.9);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn cloned_connection_shares_data() {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO contexts (id, uri, level, is_leaf, context_type, owner_user)
                 VALUES ('ctx-1', 'viking://resources/a', 0, true, 'resource', 'alice')",
                [],
            )
            .map_err(|e| CoreError::Internal(e.to_string()))?;
            Ok(())
        })
        .unwrap();

        let conn2 = db.clone_connection().unwrap();
        let count: i64 = conn2
            .query_row("SELECT COUNT(*) FROM contexts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
