use std::sync::Arc;

use duckdb::params;
use lv_core::CoreError;

use super::context::RequestContext;
use crate::llm::prompts;
use crate::llm::{ChatMessage, ChatModel, ChatRequest};
use crate::storage::{Database, EmbeddingQueue, EmbeddingTask, VikingFs};

/// Session management service.
pub struct SessionService {
    db: Arc<Database>,
    viking_fs: VikingFs,
    chat: Arc<dyn ChatModel>,
    embedding_queue: EmbeddingQueue,
}

impl SessionService {
    pub fn new(
        db: Arc<Database>,
        chat: Arc<dyn ChatModel>,
        embedding_queue: EmbeddingQueue,
    ) -> Self {
        let viking_fs = VikingFs::new(Arc::clone(&db));
        Self {
            db,
            viking_fs,
            chat,
            embedding_queue,
        }
    }

    /// Create a new session.
    pub fn create(&self, ctx: &RequestContext) -> Result<SessionInfo, CoreError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session_uri = format!("viking://session/{}/{}", ctx.owner.user_id, session_id);

        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions (session_id, owner_user, owner_account)
                 VALUES ($1, $2, $3)",
                params![session_id, ctx.owner.user_id, ctx.owner.account_id],
            )
            .map_err(|e| CoreError::Internal(format!("create session: {e}")))?;
            Ok(())
        })?;

        Ok(SessionInfo {
            session_id,
            session_uri,
            owner_user: ctx.owner.user_id.clone(),
        })
    }

    /// Get session info.
    pub fn get(&self, session_id: &str) -> Result<SessionInfo, CoreError> {
        self.db.with_conn(|conn| {
            let (owner_user,): (String,) = conn
                .query_row(
                    "SELECT owner_user FROM sessions WHERE session_id = $1",
                    params![session_id],
                    |row| Ok((row.get(0)?,)),
                )
                .map_err(|_| CoreError::NotFound(format!("session {session_id} not found")))?;

            Ok(SessionInfo {
                session_id: session_id.to_string(),
                session_uri: format!("viking://session/{}/{}", owner_user, session_id),
                owner_user,
            })
        })
    }

    /// Delete a session and its messages.
    pub fn delete(&self, session_id: &str) -> Result<(), CoreError> {
        self.db.with_conn(|conn| {
            conn.execute(
                "DELETE FROM usage_records WHERE session_id = $1",
                params![session_id],
            )
            .map_err(|e| CoreError::Internal(format!("delete usage: {e}")))?;
            conn.execute(
                "DELETE FROM messages WHERE session_id = $1",
                params![session_id],
            )
            .map_err(|e| CoreError::Internal(format!("delete messages: {e}")))?;
            conn.execute(
                "DELETE FROM sessions WHERE session_id = $1",
                params![session_id],
            )
            .map_err(|e| CoreError::Internal(format!("delete session: {e}")))?;
            Ok(())
        })
    }

    /// List all sessions.
    pub fn list(&self, ctx: &RequestContext) -> Result<Vec<SessionInfo>, CoreError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT session_id, owner_user FROM sessions
                     WHERE owner_user = $1
                     ORDER BY created_at DESC",
                )
                .map_err(|e| CoreError::Internal(format!("list sessions: {e}")))?;

            let rows = stmt
                .query_map(params![ctx.owner.user_id], |row| {
                    let sid: String = row.get(0)?;
                    let owner: String = row.get(1)?;
                    Ok(SessionInfo {
                        session_uri: format!("viking://session/{}/{}", owner, sid),
                        session_id: sid,
                        owner_user: owner,
                    })
                })
                .map_err(|e| CoreError::Internal(format!("query sessions: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CoreError::Internal(format!("collect sessions: {e}")))?;
            Ok(rows)
        })
    }

    /// Add a message to a session.
    pub fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: Option<&str>,
        parts_json: Option<&str>,
    ) -> Result<(), CoreError> {
        // Build parts JSON
        let parts = if let Some(pj) = parts_json {
            pj.to_string()
        } else if let Some(text) = content {
            serde_json::json!([{"type": "text", "text": text}]).to_string()
        } else {
            return Err(CoreError::InvalidArgument(
                "either content or parts must be provided".to_string(),
            ));
        };

        self.db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO messages (id, session_id, role, parts)
                 VALUES (nextval('messages_id_seq'), $1, $2, $3)",
                params![session_id, role, parts],
            )
            .map_err(|e| CoreError::Internal(format!("add message: {e}")))?;
            Ok(())
        })
    }

    /// Get all messages for a session.
    pub fn get_messages(&self, session_id: &str) -> Result<Vec<StoredMessage>, CoreError> {
        self.db.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT role, parts, created_at::TEXT FROM messages
                     WHERE session_id = $1 ORDER BY id",
                )
                .map_err(|e| CoreError::Internal(format!("get messages: {e}")))?;

            let rows = stmt
                .query_map(params![session_id], |row| {
                    Ok(StoredMessage {
                        role: row.get(0)?,
                        parts: row.get(1)?,
                        timestamp: row.get(2)?,
                    })
                })
                .map_err(|e| CoreError::Internal(format!("query messages: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| CoreError::Internal(format!("collect messages: {e}")))?;
            Ok(rows)
        })
    }

    /// Commit: compress old messages + extract memories.
    pub async fn commit(
        &self,
        session_id: &str,
        ctx: &RequestContext,
    ) -> Result<CommitResult, CoreError> {
        // 1. Get all messages
        let messages = self.get_messages(session_id)?;
        if messages.len() < 4 {
            return Ok(CommitResult {
                memories_extracted: 0,
            });
        }

        // 2. Format messages for LLM
        let formatted = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, extract_text_from_parts(&m.parts)))
            .collect::<Vec<_>>()
            .join("\n\n");

        // 3. Extract memories via LLM
        let extract_prompt = prompts::EXTRACT_MEMORIES.replace("{messages}", &formatted);
        let resp = self
            .chat
            .complete(ChatRequest {
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    text: extract_prompt,
                }],
                temperature: 0.3,
                max_tokens: Some(2048),
            })
            .await?;

        // 4. Parse extracted memories
        let memories = parse_extracted_memories(&resp.content);
        let mut memories_extracted = 0;

        // 5. Write each memory to the user's memory space
        for mem in &memories {
            let slug = slugify(&mem.title);
            let category = match mem.category.as_str() {
                "preference" => "preferences",
                "entity" => "entities",
                "event" => "events",
                _ => "entities",
            };
            let uri_str = format!(
                "viking://user/{}/memories/{}/{}",
                ctx.owner.user_id, category, slug
            );

            if let Ok(uri) = lv_core::uri::VikingUri::parse(&uri_str) {
                self.viking_fs
                    .write_context(&uri, &mem.title, &mem.content, true, &ctx.owner)?;
                self.viking_fs.write_content_raw(&uri, &mem.content)?;

                // Queue for embedding
                self.embedding_queue
                    .enqueue(EmbeddingTask {
                        uri: uri_str,
                        level: 0,
                        text: mem.content.clone(),
                    })
                    .await?;

                memories_extracted += 1;
            }
        }

        // 6. Update session stats
        self.db.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET stats = $1 WHERE session_id = $2",
                params![
                    serde_json::json!({"memories_extracted": memories_extracted}).to_string(),
                    session_id
                ],
            )
            .map_err(|e| CoreError::Internal(format!("update session stats: {e}")))?;
            Ok(())
        })?;

        Ok(CommitResult { memories_extracted })
    }
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub session_uri: String,
    pub owner_user: String,
}

#[derive(Debug)]
pub struct StoredMessage {
    pub role: String,
    pub parts: String, // JSON
    pub timestamp: String,
}

#[derive(Debug)]
pub struct CommitResult {
    pub memories_extracted: i64,
}

#[derive(Debug)]
struct ExtractedMemory {
    category: String,
    title: String,
    content: String,
}

fn extract_text_from_parts(parts_json: &str) -> String {
    if let Ok(parts) = serde_json::from_str::<Vec<serde_json::Value>>(parts_json) {
        parts
            .iter()
            .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        parts_json.to_string()
    }
}

fn parse_extracted_memories(llm_output: &str) -> Vec<ExtractedMemory> {
    // Try to parse JSON array from LLM output
    let trimmed = llm_output.trim();

    // Find JSON array in the response (LLM may wrap it in markdown)
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            return vec![];
        }
    } else {
        return vec![];
    };

    let parsed: Vec<serde_json::Value> = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    parsed
        .iter()
        .filter_map(|item| {
            Some(ExtractedMemory {
                category: item.get("category")?.as_str()?.to_string(),
                title: item.get("title")?.as_str()?.to_string(),
                content: item.get("content")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn slugify(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
        .chars()
        .take(60)
        .collect()
}
