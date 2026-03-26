# LiteVikings Architecture

## Goal

Drop-in Rust replacement for [OpenViking](https://github.com/volcengine/OpenViking) --
the Context Database for AI Agents. Single binary, `cargo install`, zero external
toolchain dependencies (no Go, Python, C++, CMake). Wire-compatible HTTP API so
existing Python SDK clients work unchanged.

## Guiding Principles

1. **Single binary** -- everything compiles to one executable via `cargo build --release`.
2. **gRPC-first** -- gRPC is the primary protocol between CLI and server. A single code
   path for all operations, no embedded-mode divergence.
3. **Drop-in compatible** -- HTTP/JSON gateway (`/api/v1/*`) provides backward compatibility
   with upstream OpenViking's Python SDK and existing clients.
4. **Minimal dependencies** -- prefer small, well-maintained crates. No kitchen-sink frameworks.
5. **No premature abstraction** -- start with 5 crates, split only when a boundary proves real.

## Protocol Stack

```
                     +-----------------+
                     |   Python SDK    |  (upstream, unchanged)
                     |   HTTP clients  |
                     +--------+--------+
                              |
                     HTTP/JSON (REST)
                              |
                     +--------v--------+
                     |  HTTP Gateway   |  (axum, translates REST <-> gRPC)
                     +--------+--------+
                              |
                        gRPC (protobuf)
                              |
          +-------------------+-------------------+
          |                                       |
+---------v---------+                   +---------v---------+
|    lv (CLI/TUI)   |                   |    lv-server      |
|   gRPC client     |                   |   gRPC server     |
+-------------------+                   |   + HTTP gateway   |
                                        +---------+---------+
                                                  |
                                        +---------v---------+
                                        |    lv-engine      |
                                        |   (the brain)     |
                                        +-------------------+
```

**Single code path**: both the CLI and HTTP gateway are gRPC clients. The Engine is only
accessed through the gRPC server. No "embedded mode" divergence.

## What OpenViking Does (recap)

OpenViking organizes an AI agent's context (memories, resources, skills) into a virtual
filesystem with a `viking://` URI scheme. Each node in the tree carries three tiers of
content:

| Layer | File | Tokens | Purpose |
|-------|------|--------|---------|
| L0 | `.abstract.md` | ~100 | Ultra-concise summary for vector search |
| L1 | `.overview.md` | ~1k | Navigation-oriented summary |
| L2 | Original files | Unlimited | Full content, loaded on demand |

The system provides:
- **Resource import** -- parse documents (PDF, DOCX, HTML, code, etc.) into the tree
- **Semantic generation** -- LLM-powered L0/L1 generation for imported content
- **Hierarchical retrieval** -- vector search + directory-aware reranking
- **Session management** -- conversation tracking with auto-compression and memory extraction
- **Skill registry** -- callable tool definitions stored as context nodes

## Crate Layout

```
litevikings/
  Cargo.toml              # workspace root
  crates/
    lv-core/              # Data models, URI scheme, directory presets, errors
    lv-proto/             # Protobuf definitions + generated gRPC code
    lv-engine/            # The brain: storage, LLM, services, retrieval, session, parse
    lv-server/            # gRPC server (tonic) + HTTP gateway (axum)
    lv/                   # Binary: CLI (gRPC client) + TUI + server entry point
```

5 crates. `lv-proto` is separate because both `lv-server` (server stubs) and `lv`
(client stubs) depend on it, but neither depends on each other.

### Dependency Graph

```
lv-core          (standalone -- no internal deps)
  ^
  |
lv-proto         (depends on: lv-core)
  ^
  |
lv-engine        (depends on: lv-core)
  ^
  |
lv-server        (depends on: lv-core, lv-proto, lv-engine)
  |
  |               lv-proto
  |                 ^
  |                 |
  +--------> lv   (depends on: lv-core, lv-proto, lv-server)
```

Note: `lv-engine` does NOT depend on `lv-proto`. The gRPC service implementations in
`lv-server` translate between proto types and engine types. This keeps the engine
protocol-agnostic.

### What lives where

| Module | Crate | Upstream equivalent |
|--------|-------|---------------------|
| VikingUri, Context, Owner, enums | `lv-core` | `openviking/core/context.py` |
| DirectoryPreset, preset tree | `lv-core` | `openviking/core/directories.py` |
| Error types | `lv-core` | `openviking_cli/exceptions.py` |
| Message, Part, Role | `lv-core` | `openviking/message/` |
| Protobuf service definitions | `lv-proto` | (new -- upstream is REST only) |
| Generated gRPC server/client stubs | `lv-proto` | (new) |
| DuckDB schema + connection | `lv-engine::storage` | `openviking/storage/` |
| VikingFs (filesystem abstraction) | `lv-engine::storage` | `openviking/storage/viking_fs.py` |
| EmbeddingQueue | `lv-engine::storage` | `openviking/storage/queuefs/` |
| Embedder, ChatModel, Reranker | `lv-engine::llm` | `openviking/models/` |
| Prompt templates | `lv-engine::llm` | `openviking/prompts/` |
| HierarchicalRetriever | `lv-engine::retrieve` | `openviking/retrieve/` |
| IntentAnalyzer | `lv-engine::retrieve` | `openviking/retrieve/intent_analyzer.py` |
| Session, Compressor, MemoryExtractor | `lv-engine::session` | `openviking/session/` |
| Parser trait, format parsers | `lv-engine::parse` | `openviking/parse/` |
| ImportPipeline | `lv-engine::parse` | `openviking/service/resource_service.py` |
| FSService, SearchService, etc. | `lv-engine::service` | `openviking/service/` |
| gRPC service impls | `lv-server::grpc` | (new) |
| HTTP/JSON gateway | `lv-server::http` | `openviking/server/` |
| Auth interceptor | `lv-server::auth` | `openviking/server/auth.py` |
| clap CLI (gRPC client) | `lv` | `crates/ov_cli/` |
| ratatui TUI | `lv` | `crates/ov_cli/src/tui/` |

## Key Technology Choices

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Async runtime | `tokio` | Industry standard, required by tonic/axum/reqwest |
| gRPC server | `tonic` | De facto Rust gRPC, built on hyper/tokio |
| gRPC codegen | `prost` + `tonic-build` | Standard protobuf for Rust |
| HTTP gateway | `axum` | Shares hyper with tonic, translates REST <-> gRPC |
| HTTP client | `reqwest` | For LLM API calls, rerank endpoints |
| Database | `duckdb` | Embedded OLAP DB, native vector similarity, single-file, ACID |
| CLI | `clap` (derive) | Already used in upstream Rust CLI |
| TUI | `ratatui` | Already used in upstream Rust CLI |
| Serialization | `serde` + `serde_json` + `prost` | serde for internal, prost for wire |
| PDF parsing | `pdf-extract` or `lopdf` | Pure Rust PDF text extraction |
| HTML parsing | `scraper` + `readability` | DOM parsing + article extraction |
| Markdown | `pulldown-cmark` | Commonmark parser |
| Embedding | OpenAI-compatible HTTP | No SDK dependency, just reqwest + serde |
| Logging | `tracing` | Structured, async-aware, spans for telemetry |
| Error handling | `thiserror` + `anyhow` | `thiserror` for library errors, `anyhow` in binary |
| Config | `toml` + `serde` | Simple file-based config |

## Data Flow

### 1. Resource Import (via CLI)

```
User: lv add-resource ./paper.pdf --to viking://resources/papers/my-paper
  |
  v
[lv CLI] Build AddResourceRequest protobuf
  |
  v (gRPC)
[lv-server] ResourcesService::AddResource handler
  |
  v
[lv-engine::service::ResourceService] -> ImportPipeline
  |
  v
[lv-engine::parse] Parse PDF -> extract text + structure
  |
  v
[lv-engine::storage] INSERT INTO contexts/content in DuckDB
  |
  v
[lv-engine::storage::EmbeddingQueue] Async: LLM L0/L1 + embed -> UPDATE contexts
```

### 2. Resource Import (via Python SDK / HTTP)

```
Python SDK: POST /api/v1/add-resource {"path": "...", "to": "..."}
  |
  v
[HTTP Gateway] Parse JSON -> Build AddResourceRequest protobuf
  |
  v (gRPC, in-process)
[lv-server] ResourcesService::AddResource handler  (same handler as CLI path)
  |
  v
[lv-engine] ... (identical from here)
```

### 3. Semantic Search

```
User: lv find "transformer attention mechanism" --target viking://resources
  |
  v (gRPC)
[lv-server] SearchService::Find handler
  |
  v
[lv-engine::service::SearchService] -> IntentAnalyzer -> embed query
  |
  v
[lv-engine::retrieve::HierarchicalRetriever]
  - SELECT ... array_cosine_similarity ... (global top-k)
  - Hierarchical directory walk
  - Optional rerank
  |
  v
FindResponse protobuf -> CLI formats and prints
```

### 4. Session Lifecycle

```
Agent (via HTTP SDK):
  POST /api/v1/sessions              -> Create
  POST /api/v1/sessions/{id}/messages -> AddMessage (repeated)
  POST /api/v1/sessions/{id}/commit   -> Commit (compress + extract memories)
  |
  v (HTTP gateway -> gRPC)
[lv-server] SessionsService handlers
  |
  v
[lv-engine::service::SessionService]
  -> Session.add_message() -> persist to DuckDB
  -> Session.commit() -> Compressor + MemoryExtractor -> write memories
```

## Deployment

### Running the server

```bash
lv serve --port 1933 --grpc-port 50051
```

Starts both:
- gRPC server on `0.0.0.0:50051` (for CLI and internal clients)
- HTTP gateway on `0.0.0.0:1933` (for Python SDK backward compat)

### CLI usage (always connects to server)

```bash
# Server must be running (or use --auto-start)
lv find "how does auth work"
lv ls viking://resources
lv add-resource ./docs --to viking://resources/docs --wait

# Auto-start server in background if not running
lv --auto-start find "query"

# Connect to remote server
lv --server grpc://remote-host:50051 find "query"
```

### Auto-start behavior

When `--auto-start` is set (or configured as default), the CLI:
1. Checks if a server is running on the configured gRPC port
2. If not, spawns `lv serve` as a background daemon process
3. Waits for the gRPC port to become available
4. Proceeds with the command

This gives the convenience of "embedded mode" without the architectural divergence.

## Compatibility with Upstream

| Aspect | Compatibility |
|--------|--------------|
| HTTP API (`/api/v1/*`) | Wire-compatible (same JSON request/response shapes) |
| Viking URI scheme | Identical (`viking://scope/path`) |
| L0/L1/L2 convention | Identical (`.abstract.md`, `.overview.md`) |
| Context data model | Identical ((uri, level) composite key, same fields) |
| Message model | Identical (role + parts: text/context/tool) |
| Session API | Identical (create, add_message, commit, used) |
| gRPC API | New (upstream is REST only) |
| Storage format | New (DuckDB, not AGFS) -- migration tool provided |
| Config format | New (TOML-based, simpler) |
| Python SDK | Works against HTTP gateway (unchanged) |
| Data import | `lv import-legacy` command for migrating from AGFS |

## What Gets a TODO

Features that are architecturally designed but implementation-deferred:

- **FUSE mount** (`agfs-fuse` equivalent) -- requires platform-specific code
- **Multi-modal L0/L1** (image/video/audio understanding) -- text-only first
- **External vector DB backends** (Milvus, Weaviate) -- embedded DuckDB first
- **Watch/auto-sync** for resources -- cron-based re-import
- **Bot integrations** (Telegram, Slack, DingTalk, etc.) -- separate concern
- **Encryption at rest** -- design the interface, defer implementation
- **Pack import/export** (`.ovpack` format) -- design the format, defer
- **Tree-sitter AST code parsing** -- extract function signatures + leading comments as
  individual nodes instead of dumb text chunking. Use `tree-sitter` Rust crate with C, Python,
  JS, TS, Go, Rust, Java grammars. Embed the skeleton per-function, not raw code blocks.
  (OpenViking has this via tree-sitter but only uses it for L0 summaries, not for embedding --
  their embedding still sends the full file and fails on large files. We should embed the
  skeleton directly.)
- **Gradio console** -- out of scope for Rust rewrite
- **Intent analysis via LLM** -- embed-only first, full analysis later
- **gRPC streaming** -- WaitProcessed progress, real-time session events

## File: `Cargo.toml` (workspace)

```toml
[workspace]
members = [
    "crates/lv-core",
    "crates/lv-proto",
    "crates/lv-engine",
    "crates/lv-server",
    "crates/lv",
]
resolver = "2"

[workspace.dependencies]
# Async
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# gRPC
tonic = "0.12"
prost = "0.13"
tonic-build = "0.12"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# HTTP
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
axum = { version = "0.8", features = ["json", "query"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }

# Storage
duckdb = { version = "1", features = ["bundled"] }

# CLI
clap = { version = "4", features = ["derive"] }
ratatui = "0.29"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

# Error handling
thiserror = "2"
anyhow = "1"

# Utilities
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }

[profile.release]
opt-level = 3
lto = true
strip = true
```
