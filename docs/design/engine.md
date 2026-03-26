# lv-engine: The Brain

## Overview

`lv-engine` is the monolith crate that holds all business logic. Only `lv-server`
depends on it directly -- the CLI accesses it exclusively through gRPC.
`lv-engine` has no dependency on `lv-proto` and is protocol-agnostic.
It contains six modules:

| Module | Upstream equivalent | Purpose |
|--------|---------------------|---------|
| `storage` | `openviking/storage/` | DuckDB schema, connection, VikingFs |
| `llm` | `openviking/models/` | Embedder, ChatModel, Reranker, prompts |
| `retrieve` | `openviking/retrieve/` | HierarchicalRetriever, IntentAnalyzer |
| `session` | `openviking/session/` | Session, Compressor, MemoryExtractor |
| `parse` | `openviking/parse/` | Document parsers, chunking |
| `service` | `openviking/service/` | FSService, SearchService, SessionService, ResourceService |

## Module Layout

```
lv-engine/
  src/
    lib.rs                  -- pub mod for all modules, Engine struct
    engine.rs               -- Engine: top-level entry point, owns everything

    storage/
      mod.rs
      db.rs                 -- Database: DuckDB connection pool + schema
      viking_fs.rs          -- VikingFs: filesystem abstraction over DB
      embedding_queue.rs    -- Async background embedding worker

    llm/
      mod.rs
      embedder.rs           -- Embedder trait + OpenAI-compatible impl
      chat.rs               -- ChatModel trait + OpenAI-compatible impl
      reranker.rs           -- Reranker trait + HTTP impl
      prompts.rs            -- Prompt templates (L0/L1/compress/extract)
      config.rs             -- LlmConfig

    retrieve/
      mod.rs
      hierarchical.rs       -- HierarchicalRetriever
      intent.rs             -- IntentAnalyzer
      memory_lifecycle.rs   -- Hotness scoring
      types.rs              -- TypedQuery, MatchedContext, QueryResult

    session/
      mod.rs
      session.rs            -- Session struct + lifecycle
      compressor.rs         -- SessionCompressor (LLM-powered)
      memory_extractor.rs   -- Extract structured memories from conversations
      memory_archiver.rs    -- Write memories to storage
      deduplicator.rs       -- Semantic dedup for memories

    parse/
      mod.rs
      parser.rs             -- Parser trait + dispatch
      pipeline.rs           -- ImportPipeline orchestration
      chunker.rs            -- Content chunking
      formats/
        mod.rs
        text.rs             -- Plain text / markdown
        pdf.rs              -- PDF extraction
        html.rs             -- HTML article extraction
        code.rs             -- Source code (plain text initially)
        directory.rs         -- Recursive directory import

    service/
      mod.rs
      fs_service.rs         -- Filesystem operations
      search_service.rs     -- Semantic search (find, search, grep, glob)
      session_service.rs    -- Session lifecycle management
      resource_service.rs   -- Resource import + skill registration
      relation_service.rs   -- Cross-reference management
      debug_service.rs      -- System status and diagnostics
```

## Engine (top-level entry point)

```rust
/// The Engine owns all subsystems and is shared by server and CLI.
pub struct Engine {
    pub db: Arc<Database>,
    pub viking_fs: VikingFs,
    pub embedding_queue: EmbeddingQueue,
    pub embedder: Arc<dyn Embedder>,
    pub chat: Arc<dyn ChatModel>,
    pub reranker: Option<Arc<dyn Reranker>>,
    pub retriever: HierarchicalRetriever,
    pub import_pipeline: ImportPipeline,

    // Services (the public API for server/CLI)
    pub fs: FSService,
    pub search: SearchService,
    pub sessions: SessionService,
    pub resources: ResourceService,
    pub relations: RelationService,
    pub debug: DebugService,
}

impl Engine {
    /// Initialize all subsystems from config.
    pub async fn new(config: &EngineConfig) -> Result<Self>;

    /// Graceful shutdown: flush queues, close DB.
    pub async fn shutdown(&self) -> Result<()>;
}

#[derive(Debug, Deserialize)]
pub struct EngineConfig {
    pub storage: StorageConfig,
    pub llm: LlmConfig,
}
```

---

## storage module

### DuckDB Schema

Matches upstream's data model: separate Context records per (uri, level), separate
content blobs table, sessions with messages, M:N relations.

```sql
CREATE TABLE IF NOT EXISTS contexts (
    id              TEXT PRIMARY KEY,
    uri             TEXT NOT NULL,
    parent_uri      TEXT,
    level           INTEGER,          -- 0=Abstract, 1=Overview, 2=Detail
    is_leaf         BOOLEAN NOT NULL DEFAULT FALSE,
    context_type    TEXT NOT NULL,     -- 'skill', 'memory', 'resource'
    category        TEXT DEFAULT '',
    abstract_text   TEXT DEFAULT '',
    owner_account   TEXT NOT NULL DEFAULT 'default',
    owner_user      TEXT NOT NULL,
    owner_agent     TEXT,
    session_id      TEXT,
    active_count    INTEGER DEFAULT 0,
    meta            JSON,
    vector          FLOAT[],          -- embedding (nullable, dimension set by model)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (uri, level)
);

CREATE TABLE IF NOT EXISTS content (
    key             TEXT PRIMARY KEY,  -- "{uri}/.abstract.md", "{uri}/.content", etc.
    data            BLOB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id      TEXT PRIMARY KEY,
    owner_user      TEXT NOT NULL,
    owner_account   TEXT NOT NULL DEFAULT 'default',
    compression     JSON,
    stats           JSON,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(session_id),
    role            TEXT NOT NULL,
    parts           JSON NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS relations (
    id              TEXT PRIMARY KEY,
    reason          TEXT DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS relation_uris (
    relation_id     TEXT NOT NULL REFERENCES relations(id) ON DELETE CASCADE,
    uri             TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_records (
    id              INTEGER PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(session_id),
    uri             TEXT NOT NULL,
    usage_type      TEXT NOT NULL,
    contribution    FLOAT DEFAULT 0.0,
    input           TEXT DEFAULT '',
    output          TEXT DEFAULT '',
    success         BOOLEAN DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### Database (connection + concurrency)

```rust
use duckdb::{Connection, Database as DuckDatabase};

pub struct Database {
    /// DuckDB Database handle -- can spawn multiple connections.
    inner: DuckDatabase,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let inner = DuckDatabase::open(path)?;
        let db = Self { inner };
        db.run_migrations()?;
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self> {
        let inner = DuckDatabase::open_in_memory()?;
        let db = Self { inner };
        db.run_migrations()?;
        Ok(db)
    }

    /// Get a new connection. DuckDB supports multiple concurrent readers
    /// and serializes writers automatically. Each thread/task should get
    /// its own connection.
    pub fn connect(&self) -> Result<Connection> {
        Ok(self.inner.connect()?)
    }

    fn run_migrations(&self) -> Result<()> {
        let conn = self.connect()?;
        conn.execute_batch(SCHEMA_SQL)?;
        Ok(())
    }
}
```

#### Concurrency Model

DuckDB supports multiple `Connection` instances from a single `Database` handle:
- **Multiple concurrent readers** -- parallel SELECT queries are fine.
- **Single writer** -- DuckDB serializes write transactions automatically (no deadlock,
  writers queue behind each other).
- **No external mutex needed** -- DuckDB handles locking internally.

Each tokio task (HTTP handler, embedding worker, CLI command) calls `db.connect()`
to get its own connection. This is safe because:
1. `DuckDatabase` is `Send + Sync` (thread-safe handle).
2. Each `Connection` is used by one task at a time.
3. Write serialization is handled by DuckDB's internal WAL.

```rust
// HTTP handler:
async fn handle_ls(engine: &Engine) {
    let conn = engine.db.connect()?;
    let txn = conn.transaction()?;
    // ... query ...
    txn.commit()?;
}

// Embedding worker (background):
async fn embed_worker(db: &Database, ...) {
    let conn = db.connect()?;
    let txn = conn.transaction()?;
    txn.execute("UPDATE contexts SET vector = ...", ...)?;
    txn.commit()?;
}
```

### Transaction Discipline

Per project rules, **all database interactions are wrapped in transactions**, including
reads. Every operation:
1. Gets a connection via `db.connect()`
2. Opens a transaction via `conn.transaction()`
3. Executes queries
4. Commits (or rolls back on error via `Drop`)

### Vector Search

```rust
pub fn vector_search(
    conn: &Connection,
    query: &[f32],
    scope_prefix: &str,
    limit: usize,
) -> Result<Vec<VectorMatch>> {
    let txn = conn.transaction()?;
    let mut stmt = txn.prepare(
        "SELECT uri, level, abstract_text,
                array_cosine_similarity(vector, $1::FLOAT[]) AS score
         FROM contexts
         WHERE vector IS NOT NULL
           AND uri LIKE $2
         ORDER BY score DESC
         LIMIT $3"
    )?;
    // ... execute and collect ...
}
```

DuckDB brute-force cosine similarity is fast enough for <1M vectors. Drop-in upgrade
later if needed:

```sql
CREATE INDEX idx_vectors ON contexts USING HNSW (vector) WITH (metric = 'cosine');
```

### VikingFs

Filesystem abstraction over the Database. Translates Viking URI operations to SQL.

```rust
pub struct VikingFs {
    db: Arc<Database>,
}

impl VikingFs {
    pub fn ls(&self, uri: &VikingUri, opts: &LsOptions) -> Result<Vec<DirEntry>>;
    pub fn tree(&self, uri: &VikingUri, opts: &TreeOptions) -> Result<TreeNode>;
    pub fn mkdir(&self, uri: &VikingUri, owner: &Owner) -> Result<()>;
    pub fn rm(&self, uri: &VikingUri, recursive: bool) -> Result<()>;
    pub fn mv(&self, from: &VikingUri, to: &VikingUri) -> Result<()>;
    pub fn stat(&self, uri: &VikingUri) -> Result<FileStat>;
    pub fn exists(&self, uri: &VikingUri) -> Result<bool>;

    pub fn read_content(&self, uri: &VikingUri) -> Result<String>;
    pub fn write_content(&self, uri: &VikingUri, content: &str) -> Result<()>;
    pub fn read_abstract(&self, uri: &VikingUri) -> Result<String>;
    pub fn read_overview(&self, uri: &VikingUri) -> Result<String>;

    pub fn write_context(
        &self, uri: &VikingUri, abstract_text: &str, overview: &str,
        is_leaf: bool, owner: &Owner,
    ) -> Result<()>;

    pub fn get_relations(&self, uri: &VikingUri) -> Result<Vec<Relation>>;
    pub fn add_relation(&self, relation: &Relation) -> Result<()>;
    pub fn remove_relation(&self, uri: &VikingUri, relation_id: &str) -> Result<()>;

    pub fn grep(&self, uri: &VikingUri, pattern: &str, opts: &GrepOptions) -> Result<Vec<GrepMatch>>;
    pub fn glob(&self, pattern: &str, base_uri: &VikingUri) -> Result<Vec<VikingUri>>;
}
```

### EmbeddingQueue

Async background worker. Receives texts, calls embedder, writes vectors to DuckDB.

```rust
pub struct EmbeddingQueue {
    tx: tokio::sync::mpsc::Sender<EmbeddingTask>,
}

pub struct EmbeddingTask {
    pub uri: String,
    pub level: i32,
    pub text: String,
}

impl EmbeddingQueue {
    pub fn spawn(
        db: Arc<Database>,
        embedder: Arc<dyn Embedder>,
        chat: Arc<dyn ChatModel>,
        concurrency: usize,
    ) -> (Self, tokio::task::JoinHandle<()>);

    pub async fn enqueue(&self, task: EmbeddingTask) -> Result<()>;
    pub async fn flush(&self);
}
```

Worker loop:
1. Receive tasks from channel
2. Batch up to N texts
3. Generate L0/L1 via ChatModel
4. Compute embeddings via Embedder
5. `UPDATE contexts SET abstract_text = ?, vector = ? WHERE uri = ? AND level = ?`

### Storage Config

```rust
#[derive(Debug, Deserialize)]
pub struct StorageConfig {
    pub data_dir: PathBuf,           // default: ~/.litevikings/data
    pub embedding_dimension: usize,  // must match model (e.g., 1536)
    pub embedding_concurrency: usize,
}
```

### Disk Layout

```
~/.litevikings/
  data/
    litevikings.duckdb     -- Everything: contexts, vectors, content, sessions, relations
  config.toml              -- User configuration
```

---

## llm module

Thin, provider-agnostic abstraction over LLM APIs. All communication via HTTP to
OpenAI-compatible endpoints.

### Traits

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

#[async_trait]
pub trait ChatModel: Send + Sync {
    async fn complete(&self, request: ChatRequest) -> Result<ChatResponse>;
}

#[async_trait]
pub trait Reranker: Send + Sync {
    async fn rerank(&self, query: &str, documents: &[&str], top_k: usize) -> Result<Vec<RerankResult>>;
}
```

### Implementations

- `OpenAiEmbedder` -- POST `{base_url}/embeddings`
- `OpenAiChat` -- POST `{base_url}/chat/completions`
- `HttpReranker` -- POST `{base_url}/rerank`

All use `reqwest` with `rustls`. Works with OpenAI, Azure, Volcengine, Ollama, vLLM,
and any OpenAI-compatible provider.

### Prompt Templates

```rust
pub mod prompts {
    pub const GENERATE_ABSTRACT: &str = "...";  // L0 from content (~100 tokens)
    pub const GENERATE_OVERVIEW: &str = "...";  // L1 from content (~1k tokens)
    pub const COMPRESS_SESSION: &str = "...";   // Compress conversation
    pub const EXTRACT_MEMORIES: &str = "...";   // Extract preferences/entities/events
    pub const DIRECTORY_ABSTRACT: &str = "...";  // L0 from child abstracts
}
```

### Config

```rust
#[derive(Debug, Deserialize)]
pub struct LlmConfig {
    pub embedder: EmbedderConfig,
    pub chat: ChatConfig,
    pub rerank: Option<RerankConfig>,
}
```

---

## retrieve module

Hierarchical retrieval -- the core differentiation from flat vector search.

### Algorithm

```
1. GLOBAL SEARCH
   - Embed query -> vector
   - SELECT ... array_cosine_similarity ... (top GLOBAL_SEARCH_TOPK = 5)
   - Collect matching URIs and their parent directories

2. DIRECTORY SCORING
   - For each parent directory:
     a. Load children's L0 abstracts
     b. Score via reranker (or cosine fallback)
     c. If dir score > max_child * DIRECTORY_DOMINANCE_RATIO -> include dir
     d. Otherwise -> expand children that pass threshold

3. RECURSIVE EXPANSION (max MAX_CONVERGENCE_ROUNDS = 3)
   - Expand top-scoring unexpanded directories
   - Stop when no new results or max rounds reached

4. RELATION ENRICHMENT
   - For top-k results, load relations (max MAX_RELATIONS = 5)

5. FINAL RANKING
   - combined_score = (1 - HOTNESS_ALPHA) * retrieval_score + HOTNESS_ALPHA * hotness
   - Return top-k
```

### Interface

```rust
pub struct HierarchicalRetriever {
    db: Arc<Database>,
    embedder: Arc<dyn Embedder>,
    reranker: Option<Arc<dyn Reranker>>,
    threshold: f32,
}

impl HierarchicalRetriever {
    pub async fn retrieve(
        &self, query: &TypedQuery, owner: &Owner,
        limit: usize, mode: RetrieverMode, score_threshold: Option<f32>,
    ) -> Result<QueryResult>;
}
```

### IntentAnalyzer

```rust
pub struct IntentAnalyzer {
    chat: Arc<dyn ChatModel>,
    embedder: Arc<dyn Embedder>,
}

impl IntentAnalyzer {
    /// Full analysis: LLM rewrites query + determines target types.
    pub async fn analyze(&self, query: &str, context_hint: Option<&str>) -> Result<TypedQuery>;

    /// Simple mode: embed query directly (no LLM call). Used initially.
    pub async fn embed_only(&self, query: &str) -> Result<TypedQuery>;
}
```

### Hotness (memory lifecycle)

```rust
pub fn hotness_score(active_count: u64, updated_at: DateTime<Utc>) -> f32 {
    let days = (Utc::now() - updated_at).num_days().max(0) as f32;
    let raw = active_count as f32 / (1.0 + days);
    1.0 / (1.0 + (-raw).exp())  // sigmoid
}
```

---

## session module

### Session

```rust
pub struct Session {
    pub session_id: String,
    pub owner: Owner,
    pub session_uri: VikingUri,
    pub created_at: DateTime<Utc>,
    messages: Vec<Message>,
    usage_records: Vec<Usage>,
    compression: SessionCompression,
    stats: SessionStats,
    auto_commit_threshold: u32,
}

impl Session {
    pub async fn add_message(&mut self, msg: Message, db: &Database) -> Result<()>;
    pub async fn commit(
        &mut self, db: &Database, compressor: &SessionCompressor,
        archiver: &MemoryArchiver,
    ) -> Result<CommitResult>;
    pub fn get_messages(&self) -> &[Message];
    pub fn summary(&self) -> &str;
}
```

### SessionCompressor

```rust
pub struct SessionCompressor {
    chat: Arc<dyn ChatModel>,
    keep_recent: usize,  // rounds to keep uncompressed
}

impl SessionCompressor {
    pub async fn compress(&self, messages: &[Message]) -> Result<CompressResult>;
    pub async fn extract_memories(&self, messages: &[Message]) -> Result<Vec<ExtractedMemory>>;
}
```

### MemoryArchiver

Writes extracted memories to `viking://user/{user}/memories/{category}/{slug}`.

```rust
pub struct MemoryArchiver {
    db: Arc<Database>,
    embedding_queue: EmbeddingQueue,
    deduplicator: MemoryDeduplicator,
}

impl MemoryArchiver {
    pub async fn archive(&self, memories: &[ExtractedMemory], owner: &Owner) -> Result<Vec<VikingUri>>;
}
```

### MemoryDeduplicator

Uses vector search on the memories scope to detect semantic duplicates.

```rust
pub struct MemoryDeduplicator {
    db: Arc<Database>,
    embedder: Arc<dyn Embedder>,
    similarity_threshold: f32,  // e.g., 0.92
}

impl MemoryDeduplicator {
    pub async fn is_duplicate(&self, memory: &ExtractedMemory, owner: &Owner) -> Result<bool> {
        let embedding = self.embedder.embed(&memory.title).await?;
        let conn = self.db.connect()?;
        let scope = format!("viking://user/{}/memories", owner.user_space_name());
        let matches = storage::vector_search(&conn, &embedding, &scope, 1)?;
        Ok(matches.first().map_or(false, |m| m.score >= self.similarity_threshold))
    }
}
```

---

## parse module

### Parser Trait

```rust
pub trait Parser: Send + Sync {
    fn can_parse(&self, input: &ParseInput) -> bool;
    fn parse(&self, input: &ParseInput) -> Result<ParseOutput>;
}
```

### Format Parsers

| Parser | Formats | Status |
|--------|---------|--------|
| `TextParser` | .txt, .md, .rst | Planned |
| `PdfParser` | .pdf | Planned |
| `HtmlParser` | .html, .htm, URLs | Planned |
| `CodeParser` | .rs, .py, .js, .ts, .go, .java, .cpp, .c | Planned (plain text) |
| `DirectoryParser` | directories | Planned |
| `DocxParser` | .docx | TODO |
| `PptxParser` | .pptx | TODO |

### ImportPipeline

Orchestrates: fetch -> parse -> build tree in DB -> queue L0/L1 generation.

```rust
pub struct ImportPipeline {
    parsers: Vec<Box<dyn Parser>>,
    db: Arc<Database>,
    embedding_queue: EmbeddingQueue,
}

impl ImportPipeline {
    pub async fn import(&self, req: &ImportRequest) -> Result<ImportResult> {
        let input = self.fetch(&req.source).await?;
        let parsed = self.find_parser(&input)?.parse(&input)?;
        let nodes_created = self.build_tree(&req.target_uri, &parsed.nodes, &req.owner)?;
        let queued = self.queue_semantic_generation(&req.target_uri, &parsed.nodes).await?;
        if req.wait {
            self.embedding_queue.flush().await;
        }
        Ok(ImportResult { root_uri: req.target_uri.clone(), nodes_created, processing_queued: queued })
    }
}
```

`build_tree` writes to DuckDB:
1. INSERT INTO content (key, data) for each node's raw text
2. INSERT INTO contexts for metadata + parent_uri
3. No separate `add_child` call -- parent_uri column IS the tree structure

---

## service module

The service layer matches upstream's `openviking/service/`. Each service holds
references to the subsystems it needs and exposes the operations that HTTP routers
and CLI commands call.

### FSService

```rust
pub struct FSService {
    viking_fs: VikingFs,
    db: Arc<Database>,
}

impl FSService {
    pub fn ls(&self, uri: &str, ctx: &RequestContext, opts: &LsOptions) -> Result<serde_json::Value>;
    pub fn tree(&self, uri: &str, ctx: &RequestContext, opts: &TreeOptions) -> Result<serde_json::Value>;
    pub fn mkdir(&self, uri: &str, ctx: &RequestContext) -> Result<()>;
    pub fn rm(&self, uri: &str, ctx: &RequestContext, recursive: bool) -> Result<()>;
    pub fn mv(&self, from: &str, to: &str, ctx: &RequestContext) -> Result<()>;
    pub fn stat(&self, uri: &str, ctx: &RequestContext) -> Result<serde_json::Value>;
    pub fn read(&self, uri: &str, ctx: &RequestContext) -> Result<String>;
    pub fn abstract_text(&self, uri: &str, ctx: &RequestContext) -> Result<String>;
    pub fn overview(&self, uri: &str, ctx: &RequestContext) -> Result<String>;
    pub fn write(&self, uri: &str, content: &str, ctx: &RequestContext) -> Result<()>;
}
```

### SearchService

```rust
pub struct SearchService {
    retriever: HierarchicalRetriever,
    analyzer: IntentAnalyzer,
    viking_fs: VikingFs,
}

impl SearchService {
    /// Semantic search (no session context). Maps to POST /api/v1/search/find.
    pub async fn find(
        &self, query: &str, target_uri: Option<&str>, ctx: &RequestContext,
        limit: usize, score_threshold: Option<f32>,
    ) -> Result<QueryResult>;

    /// Search with session context. Maps to POST /api/v1/search/search.
    pub async fn search(
        &self, query: &str, target_uri: Option<&str>, session_id: Option<&str>,
        ctx: &RequestContext, limit: usize, score_threshold: Option<f32>,
    ) -> Result<QueryResult>;

    /// Text pattern search. Maps to POST /api/v1/search/grep.
    pub fn grep(&self, uri: &str, pattern: &str, ctx: &RequestContext) -> Result<Vec<GrepMatch>>;

    /// URI pattern matching. Maps to POST /api/v1/search/glob.
    pub fn glob(&self, pattern: &str, uri: &str, ctx: &RequestContext) -> Result<Vec<VikingUri>>;
}
```

### SessionService

```rust
pub struct SessionService {
    db: Arc<Database>,
    compressor: SessionCompressor,
    archiver: MemoryArchiver,
}

impl SessionService {
    pub async fn create(&self, ctx: &RequestContext) -> Result<Session>;
    pub async fn get(&self, session_id: &str, ctx: &RequestContext) -> Result<Session>;
    pub async fn delete(&self, session_id: &str, ctx: &RequestContext) -> Result<()>;
    pub async fn list(&self, ctx: &RequestContext) -> Result<Vec<SessionSummary>>;
    pub async fn add_message(
        &self, session_id: &str, msg: Message, ctx: &RequestContext,
    ) -> Result<()>;
    pub async fn commit(&self, session_id: &str, ctx: &RequestContext) -> Result<CommitResult>;
    pub async fn get_messages(&self, session_id: &str, ctx: &RequestContext) -> Result<Vec<Message>>;
    pub async fn record_usage(
        &self, session_id: &str, usage: Usage, ctx: &RequestContext,
    ) -> Result<()>;
}
```

### ResourceService

```rust
pub struct ResourceService {
    import_pipeline: ImportPipeline,
    viking_fs: VikingFs,
    embedding_queue: EmbeddingQueue,
}

impl ResourceService {
    /// Import a file, URL, or directory. Maps to POST /api/v1/add-resource.
    pub async fn add_resource(
        &self, req: &AddResourceRequest, ctx: &RequestContext,
    ) -> Result<ImportResult>;

    /// Register a skill definition. Maps to POST /api/v1/add-skill.
    pub async fn add_skill(
        &self, req: &AddSkillRequest, ctx: &RequestContext,
    ) -> Result<()>;

    /// Wait for all pending semantic generation to complete.
    pub async fn wait_processed(&self) -> Result<()> {
        self.embedding_queue.flush().await;
        Ok(())
    }
}
```

### RelationService

```rust
pub struct RelationService {
    viking_fs: VikingFs,
}

impl RelationService {
    pub fn get_relations(&self, uri: &str, ctx: &RequestContext) -> Result<Vec<Relation>>;
    pub fn link(&self, uris: &[String], reason: &str, ctx: &RequestContext) -> Result<Relation>;
    pub fn unlink(&self, uri: &str, relation_id: &str, ctx: &RequestContext) -> Result<()>;
}
```

### DebugService

```rust
pub struct DebugService {
    db: Arc<Database>,
}

impl DebugService {
    pub fn status(&self) -> Result<SystemStatus>;
}

pub struct SystemStatus {
    pub context_count: u64,
    pub session_count: u64,
    pub vector_count: u64,
    pub db_size_bytes: u64,
}
```

### RequestContext

Passed through from HTTP auth or CLI config. Matches upstream.

```rust
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
```

---

## Crate Dependencies

```toml
[dependencies]
lv-core = { path = "../lv-core" }

tokio = { workspace = true }
async-trait = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
toml = { workspace = true }
reqwest = { workspace = true }
duckdb = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }

# Parse
pdf-extract = "0.7"
scraper = "0.21"
pulldown-cmark = "0.12"
walkdir = "2"
```

---

## TODO / Deferred

- **HNSW index**: `CREATE INDEX ... USING HNSW` for >1M vector scale
- **Multi-modal**: VLM trait for image/video understanding
- **Streaming chat**: Not needed for internal generation
- **Token counting**: Client-side estimation for budget management
- **Rate limiting**: Backoff for LLM provider rate limits
- **Sparse vectors**: BM25-style hybrid search
- **Intent analysis via LLM**: Currently embed_only; full analysis deferred
- **Score propagation**: Between parent/child directories
- **Retrieval telemetry**: Trajectory visualization data
- **Tree-sitter code parsing**: Structured function/class extraction
- **Office formats**: DOCX, PPTX, XLSX parsers
- **Legacy migration**: `lv import-legacy --agfs-path /path/to/agfs/data`
- **WAL tuning**: DuckDB WAL configuration for server mode concurrency
