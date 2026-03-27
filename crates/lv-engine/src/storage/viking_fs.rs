use std::sync::Arc;

use duckdb::params;
use lv_core::error::CoreError;
use lv_core::owner::Owner;
use lv_core::uri::VikingUri;

use super::db::Database;

/// Directory listing entry.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub uri: String,
    pub is_leaf: bool,
    pub abstract_text: String,
    pub context_type: String,
    pub updated_at: String,
}

/// Options for `ls` operation.
#[derive(Debug, Default)]
pub struct LsOptions {
    pub simple: bool,
    pub recursive: bool,
    pub node_limit: i32,
}

/// Filesystem abstraction over DuckDB.
///
/// Translates Viking URI operations into SQL queries.
pub struct VikingFs {
    db: Arc<Database>,
}

impl VikingFs {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// List children of a URI.
    pub fn ls(&self, uri: &VikingUri, opts: &LsOptions) -> Result<Vec<DirEntry>, CoreError> {
        let limit = if opts.node_limit > 0 {
            opts.node_limit
        } else {
            1000
        };

        self.db.with_conn(|conn| {
            if opts.recursive {
                let mut stmt = conn
                    .prepare(
                        "SELECT uri, is_leaf, abstract_text, context_type, updated_at::TEXT
                         FROM contexts
                         WHERE (parent_uri = $1 OR uri LIKE $2) AND level = 0
                         ORDER BY uri
                         LIMIT $3",
                    )
                    .map_err(|e| CoreError::Internal(format!("prepare ls: {e}")))?;

                let prefix = format!("{}/%", uri.as_str());
                let rows = stmt
                    .query_map(params![uri.as_str(), prefix, limit], |row| {
                        Ok(DirEntry {
                            uri: row.get(0)?,
                            is_leaf: row.get(1)?,
                            abstract_text: row.get(2)?,
                            context_type: row.get(3)?,
                            updated_at: row.get(4)?,
                        })
                    })
                    .map_err(|e| CoreError::Internal(format!("query ls: {e}")))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| CoreError::Internal(format!("collect ls: {e}")))?;
                Ok(rows)
            } else {
                let mut stmt = conn
                    .prepare(
                        "SELECT uri, is_leaf, abstract_text, context_type, updated_at::TEXT
                         FROM contexts
                         WHERE parent_uri = $1 AND level = 0
                         ORDER BY uri
                         LIMIT $2",
                    )
                    .map_err(|e| CoreError::Internal(format!("prepare ls: {e}")))?;

                let rows = stmt
                    .query_map(params![uri.as_str(), limit], |row| {
                        Ok(DirEntry {
                            uri: row.get(0)?,
                            is_leaf: row.get(1)?,
                            abstract_text: row.get(2)?,
                            context_type: row.get(3)?,
                            updated_at: row.get(4)?,
                        })
                    })
                    .map_err(|e| CoreError::Internal(format!("query ls: {e}")))?
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| CoreError::Internal(format!("collect ls: {e}")))?;
                Ok(rows)
            }
        })
    }

    /// Create a directory (non-leaf context node).
    pub fn mkdir(&self, uri: &VikingUri, owner: &Owner) -> Result<(), CoreError> {
        let id = uuid::Uuid::new_v4().to_string();
        let parent_uri = uri.parent().map(|p| p.as_str().to_string());
        let context_type = uri.derive_context_type();
        let category = uri.derive_category();

        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO contexts (id, uri, parent_uri, level, is_leaf, context_type, category, owner_account, owner_user, owner_agent)
                 VALUES ($1, $2, $3, 0, false, $4, $5, $6, $7, $8)
                 ON CONFLICT (uri, level) DO NOTHING",
                params![
                    id,
                    uri.as_str(),
                    parent_uri,
                    context_type.as_str(),
                    category.as_str(),
                    owner.account_id,
                    owner.user_id,
                    owner.agent_name,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("mkdir: {e}")))?;

            Ok(())
        })
    }

    /// Remove a URI and optionally all descendants.
    pub fn rm(&self, uri: &VikingUri, recursive: bool) -> Result<u64, CoreError> {
        self.db.with_conn(|conn| {
            if recursive {
                let prefix = format!("{}/%", uri.as_str());
                conn.execute(
                    "DELETE FROM content WHERE key LIKE $1 OR key LIKE $2",
                    params![
                        format!("{}/%", uri.as_str()),
                        format!("{}/.%", uri.as_str())
                    ],
                )
                .map_err(|e| CoreError::Internal(format!("rm content: {e}")))?;

                let deleted = conn
                    .execute(
                        "DELETE FROM contexts WHERE uri = $1 OR uri LIKE $2",
                        params![uri.as_str(), prefix],
                    )
                    .map_err(|e| CoreError::Internal(format!("rm contexts: {e}")))?;

                Ok(deleted as u64)
            } else {
                conn.execute(
                    "DELETE FROM content WHERE key LIKE $1",
                    params![format!("{}/.%", uri.as_str())],
                )
                .map_err(|e| CoreError::Internal(format!("rm content: {e}")))?;

                let deleted = conn
                    .execute("DELETE FROM contexts WHERE uri = $1", params![uri.as_str()])
                    .map_err(|e| CoreError::Internal(format!("rm contexts: {e}")))?;

                Ok(deleted as u64)
            }
        })
    }

    /// Move/rename a URI and all descendants.
    pub fn mv(&self, from: &VikingUri, to: &VikingUri) -> Result<(), CoreError> {
        let prefix = format!("{}/%", from.as_str());

        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE contexts SET
                    uri = replace(uri, $1, $2),
                    parent_uri = replace(parent_uri, $1, $2),
                    updated_at = now()
                 WHERE uri = $1 OR uri LIKE $3",
                params![from.as_str(), to.as_str(), prefix],
            )
            .map_err(|e| CoreError::Internal(format!("mv contexts: {e}")))?;

            conn.execute(
                "UPDATE content SET key = replace(key, $1, $2)
                 WHERE key LIKE $3 OR key LIKE $4",
                params![
                    from.as_str(),
                    to.as_str(),
                    format!("{}/%", from.as_str()),
                    format!("{}/.%", from.as_str())
                ],
            )
            .map_err(|e| CoreError::Internal(format!("mv content: {e}")))?;

            Ok(())
        })
    }

    /// Check if a URI exists.
    pub fn exists(&self, uri: &VikingUri) -> Result<bool, CoreError> {
        self.db.with_conn(|conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM contexts WHERE uri = $1",
                    params![uri.as_str()],
                    |row| row.get(0),
                )
                .map_err(|e| CoreError::Internal(format!("exists: {e}")))?;
            Ok(count > 0)
        })
    }

    /// Read L0 abstract text.
    pub fn read_abstract(&self, uri: &VikingUri) -> Result<String, CoreError> {
        self.db.with_conn(|conn| {
            // Try context table first (abstract_text field)
            let result: Result<String, _> = conn.query_row(
                "SELECT abstract_text FROM contexts WHERE uri = $1 AND level = 0",
                params![uri.as_str()],
                |row| row.get(0),
            );

            match result {
                Ok(text) if !text.is_empty() => Ok(text),
                _ => {
                    // Fallback to content table
                    let key = uri.abstract_key();
                    let data: Result<Vec<u8>, _> = conn.query_row(
                        "SELECT data FROM content WHERE key = $1",
                        params![key],
                        |row| row.get(0),
                    );
                    match data {
                        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).to_string()),
                        Err(_) => Err(CoreError::NotFound(format!(
                            "no abstract for {}",
                            uri.as_str()
                        ))),
                    }
                }
            }
        })
    }

    /// Read L1 overview text.
    pub fn read_overview(&self, uri: &VikingUri) -> Result<String, CoreError> {
        self.db.with_conn(|conn| {
            let key = uri.overview_key();
            let data: Result<Vec<u8>, _> = conn.query_row(
                "SELECT data FROM content WHERE key = $1",
                params![key],
                |row| row.get(0),
            );
            match data {
                Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).to_string()),
                Err(_) => Err(CoreError::NotFound(format!(
                    "no overview for {}",
                    uri.as_str()
                ))),
            }
        })
    }

    /// Read L2 content.
    pub fn read_content(&self, uri: &VikingUri) -> Result<String, CoreError> {
        self.db.with_conn(|conn| {
            let key = uri.content_key();
            let data: Result<Vec<u8>, _> = conn.query_row(
                "SELECT data FROM content WHERE key = $1",
                params![key],
                |row| row.get(0),
            );
            match data {
                Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).to_string()),
                Err(_) => Err(CoreError::NotFound(format!(
                    "no content for {}",
                    uri.as_str()
                ))),
            }
        })
    }

    /// Write a context node with L0 abstract and L1 overview.
    pub fn write_context(
        &self,
        uri: &VikingUri,
        abstract_text: &str,
        overview: &str,
        is_leaf: bool,
        owner: &Owner,
    ) -> Result<(), CoreError> {
        let id = uuid::Uuid::new_v4().to_string();
        let parent_uri = uri.parent().map(|p| p.as_str().to_string());
        let context_type = uri.derive_context_type();
        let category = uri.derive_category();

        self.db.with_conn(|conn| {
            // Upsert L0 context record
            conn.execute(
                "INSERT INTO contexts (id, uri, parent_uri, level, is_leaf, context_type, category, abstract_text, owner_account, owner_user, owner_agent)
                 VALUES ($1, $2, $3, 0, $4, $5, $6, $7, $8, $9, $10)
                 ON CONFLICT (uri, level) DO UPDATE SET
                    abstract_text = EXCLUDED.abstract_text,
                    is_leaf = EXCLUDED.is_leaf,
                    updated_at = now()",
                params![
                    id,
                    uri.as_str(),
                    parent_uri,
                    is_leaf,
                    context_type.as_str(),
                    category.as_str(),
                    abstract_text,
                    owner.account_id,
                    owner.user_id,
                    owner.agent_name,
                ],
            )
            .map_err(|e| CoreError::Internal(format!("write context L0: {e}")))?;

            // Write L0 content blob
            conn.execute(
                "INSERT INTO content (key, data) VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data",
                params![uri.abstract_key(), abstract_text.as_bytes()],
            )
            .map_err(|e| CoreError::Internal(format!("write abstract blob: {e}")))?;

            // Write L1 content blob
            if !overview.is_empty() {
                conn.execute(
                    "INSERT INTO content (key, data) VALUES ($1, $2)
                     ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data",
                    params![uri.overview_key(), overview.as_bytes()],
                )
                .map_err(|e| CoreError::Internal(format!("write overview blob: {e}")))?;
            }

            Ok(())
        })
    }

    /// Check if content has changed since last write by comparing hash.
    /// Returns true if content is new or changed, false if unchanged.
    pub fn content_changed(&self, uri: &VikingUri, content: &str) -> Result<bool, CoreError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let new_hash = format!("{:x}", hasher.finish());

        self.db.with_conn(|conn| {
            let existing: Result<String, _> = conn.query_row(
                "SELECT content_hash FROM contexts WHERE uri = $1 AND level = 0",
                params![uri.as_str()],
                |row| row.get(0),
            );
            match existing {
                Ok(old_hash) => Ok(old_hash != new_hash),
                Err(_) => Ok(true), // no existing record = new content
            }
        })
    }

    /// Update the content hash for a URI.
    pub fn set_content_hash(&self, uri: &VikingUri, content: &str) -> Result<(), CoreError> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let hash = format!("{:x}", hasher.finish());

        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE contexts SET content_hash = $1 WHERE uri = $2 AND level = 0",
                params![hash, uri.as_str()],
            )
            .map_err(|e| CoreError::Internal(format!("set content_hash: {e}")))?;
            Ok(())
        })
    }

    /// Write raw L2 content.
    pub fn write_content_raw(&self, uri: &VikingUri, content: &str) -> Result<(), CoreError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO content (key, data) VALUES ($1, $2)
                 ON CONFLICT (key) DO UPDATE SET data = EXCLUDED.data",
                params![uri.content_key(), content.as_bytes()],
            )
            .map_err(|e| CoreError::Internal(format!("write content: {e}")))?;
            Ok(())
        })
    }

    /// Vector similarity search.
    pub fn vector_search(
        &self,
        query: &[f32],
        scope_prefix: &str,
        limit: usize,
    ) -> Result<Vec<VectorMatch>, CoreError> {
        // Build query vector as SQL literal
        let vec_str = query
            .iter()
            .map(|f| f.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT uri, level, abstract_text,
                    list_cosine_similarity(vector, [{vec_str}]) AS score
             FROM contexts
             WHERE vector IS NOT NULL
               AND uri LIKE $1
             ORDER BY score DESC
             LIMIT $2"
        );

        let prefix = format!("{scope_prefix}%");

        self.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| CoreError::Internal(format!("prepare vector_search: {e}")))?;

            let rows = stmt
                .query_map(params![prefix, limit as i64], |row| {
                    Ok(VectorMatch {
                        uri: row.get(0)?,
                        level: row.get(1)?,
                        abstract_text: row.get(2)?,
                        score: row.get(3)?,
                    })
                })
                .map_err(|e| CoreError::Internal(format!("query vector_search: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CoreError::Internal(format!("collect vector_search: {e}")))?;

            Ok(rows)
        })
    }
}

/// A match from vector similarity search.
#[derive(Debug, Clone)]
pub struct VectorMatch {
    pub uri: String,
    pub level: i32,
    pub abstract_text: String,
    pub score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Arc<Database> {
        Arc::new(Database::open_in_memory().unwrap())
    }

    #[test]
    fn mkdir_and_ls() {
        let db = test_db();
        let fs = VikingFs::new(db);
        let owner = Owner::default();

        let root = VikingUri::parse("viking://resources").unwrap();
        let docs = VikingUri::parse("viking://resources/docs").unwrap();
        let papers = VikingUri::parse("viking://resources/papers").unwrap();

        fs.mkdir(&root, &owner).unwrap();
        fs.mkdir(&docs, &owner).unwrap();
        fs.mkdir(&papers, &owner).unwrap();

        let entries = fs.ls(&root, &LsOptions::default()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].uri, "viking://resources/docs");
        assert_eq!(entries[1].uri, "viking://resources/papers");
    }

    #[test]
    fn write_and_read_context() {
        let db = test_db();
        let fs = VikingFs::new(db);
        let owner = Owner::default();
        let uri = VikingUri::parse("viking://resources/docs/readme").unwrap();

        fs.write_context(
            &uri,
            "A readme file",
            "Full overview of the readme",
            true,
            &owner,
        )
        .unwrap();

        let abs = fs.read_abstract(&uri).unwrap();
        assert_eq!(abs, "A readme file");

        let overview = fs.read_overview(&uri).unwrap();
        assert_eq!(overview, "Full overview of the readme");
    }

    #[test]
    fn write_and_read_content() {
        let db = test_db();
        let fs = VikingFs::new(db);
        let uri = VikingUri::parse("viking://resources/docs/readme").unwrap();

        fs.write_content_raw(&uri, "# Hello World\n\nThis is the readme.")
            .unwrap();

        let content = fs.read_content(&uri).unwrap();
        assert_eq!(content, "# Hello World\n\nThis is the readme.");
    }

    #[test]
    fn rm_recursive() {
        let db = test_db();
        let fs = VikingFs::new(db);
        let owner = Owner::default();

        let root = VikingUri::parse("viking://resources").unwrap();
        let docs = VikingUri::parse("viking://resources/docs").unwrap();
        let readme = VikingUri::parse("viking://resources/docs/readme").unwrap();

        fs.mkdir(&root, &owner).unwrap();
        fs.mkdir(&docs, &owner).unwrap();
        fs.write_context(&readme, "readme", "", true, &owner)
            .unwrap();

        assert!(fs.exists(&docs).unwrap());
        assert!(fs.exists(&readme).unwrap());

        fs.rm(&docs, true).unwrap();

        assert!(!fs.exists(&docs).unwrap());
        assert!(!fs.exists(&readme).unwrap());
        assert!(fs.exists(&root).unwrap());
    }

    #[test]
    fn mv_uri() {
        let db = test_db();
        let fs = VikingFs::new(db);
        let owner = Owner::default();

        let old = VikingUri::parse("viking://resources/old-name").unwrap();
        let new = VikingUri::parse("viking://resources/new-name").unwrap();

        fs.write_context(&old, "some doc", "overview", true, &owner)
            .unwrap();
        assert!(fs.exists(&old).unwrap());

        fs.mv(&old, &new).unwrap();

        assert!(!fs.exists(&old).unwrap());
        assert!(fs.exists(&new).unwrap());
        let abs = fs.read_abstract(&new).unwrap();
        assert_eq!(abs, "some doc");
    }

    #[test]
    fn vector_search_through_fs() {
        let db = test_db();
        let fs = VikingFs::new(Arc::clone(&db));

        // Insert contexts with vectors directly via DB
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO contexts (id, uri, parent_uri, level, is_leaf, context_type, owner_user, abstract_text, vector)
                 VALUES ('v1', 'viking://resources/rust', 'viking://resources', 0, true, 'resource', 'default', 'Rust programming', [1.0, 0.0, 0.0])",
                [],
            ).map_err(|e| CoreError::Internal(format!("insert rust: {e}")))?;
            conn.execute(
                "INSERT INTO contexts (id, uri, parent_uri, level, is_leaf, context_type, owner_user, abstract_text, vector)
                 VALUES ('v2', 'viking://resources/python', 'viking://resources', 0, true, 'resource', 'default', 'Python programming', [0.0, 1.0, 0.0])",
                [],
            ).map_err(|e| CoreError::Internal(format!("insert python: {e}")))?;
            Ok(())
        }).unwrap();

        let results = fs
            .vector_search(&[1.0, 0.0, 0.0], "viking://resources", 5)
            .unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].uri, "viking://resources/rust");
        assert!(results[0].score > 0.99);
    }
}
