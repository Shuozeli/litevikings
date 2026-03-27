# CLAUDE.md -- LiteVikings Project Context

## What This Is

LiteVikings is a drop-in Rust replacement for [OpenViking](https://github.com/volcengine/OpenViking) -- a Context Database for AI Agents. Single binary, gRPC-first, DuckDB storage.

## Crate Layout

```
crates/
  lv-core/      -- Data models (VikingUri, Context, Owner, Message, presets, errors). No async.
  lv-proto/     -- Protobuf definitions + tonic codegen. 7 proto files, 6 gRPC services.
  lv-engine/    -- The brain. Storage (DuckDB), LLM clients, parsing, retrieval, sessions.
  lv-server/    -- Thin gRPC server (tonic). Translates proto types to engine calls.
  lv/           -- Binary. CLI (clap) + gRPC client. Commands: serve, setup, add-resource, find, etc.
```

Dependency graph: `lv-core <- lv-proto <- lv-server <- lv` and `lv-core <- lv-engine <- lv-server`.
Engine does NOT depend on proto (protocol-agnostic).

## Build & Test

```bash
# Prerequisites: Rust toolchain + protoc
# macOS: brew install protobuf
# Linux: sudo apt install protobuf-compiler

cargo build --release -p lv    # ~3 min (DuckDB C++ compile)
cargo test --workspace          # 29 tests
cargo clippy --workspace -- -D warnings
cargo fmt --all --check
```

CI runs check, test, fmt, clippy on every push via `.github/workflows/ci.yml`.

## Running

```bash
# First time setup (installs Ollama + pulls models):
lv setup

# Start server (all LLM config is REQUIRED, no defaults):
lv serve \
    --llm-base-url http://localhost:11434/v1 \
    --chat-model qwen2.5:3b \
    --embed-model nomic-embed-text \
    --embed-dim 768

# Or via env vars:
export LV_LLM_BASE_URL=http://localhost:11434/v1
export LV_CHAT_MODEL=qwen2.5:3b
export LV_EMBED_MODEL=nomic-embed-text
export LV_EMBED_DIM=768
lv serve
```

## Mac Mini Deployment

Primary server: `10.0.0.56` (Apple M4, 16GB). See `.private/mac-mini-setup.md` for credentials (gitignored).

```bash
# Deploy latest:
sshpass -p 'yuanchenxi' ssh chenxiyuan@10.0.0.56
cd ~/projects/litevikings && git pull && cargo build --release -p lv
cp target/release/lv ~/.cargo/bin/

# Ollama must listen on 0.0.0.0:
OLLAMA_HOST=0.0.0.0 nohup /opt/homebrew/opt/ollama/bin/ollama serve &
```

## Key Design Decisions

- **DuckDB single file** -- all data (contexts, vectors, content, sessions) in one `.duckdb` file
- **gRPC only** -- no HTTP gateway, no Python SDK. CLI is a pure gRPC client.
- **No default config** -- `lv serve` fails fast if LLM config is missing. Per project rule.
- **Brute-force vector search** -- `list_cosine_similarity()` without HNSW index. Fast enough for <100K vectors. HNSW removed because the vss extension downloads 50MB on first run and blocks startup.
- **Dedicated tokio runtime for embedding worker** -- avoids deadlock from `spawn_blocking` + `block_on` on main runtime.
- **UTF-8 safe truncation** -- always use `is_char_boundary()` before slicing strings.
- **Markdown parser splits on H1/H2 only** -- H3+ stays in parent section. Merge sections <150 chars. Cap at 30 nodes per file. Prevents API reference explosion (1628 nodes -> 2).

## Common Pitfalls

1. **Never add network calls to startup path** -- the HNSW vss download bug blocked server for minutes.
2. **Never hardcode endpoints** -- LLM URLs, credentials go in env vars or `.private/`.
3. **String slicing** -- `&text[..n]` panics on multi-byte UTF-8. Use `truncate_text()` helper.
4. **DuckDB Connection is !Send** -- wrap in `Mutex` for `Arc<Database>` to be `Sync`. Use `db.with_conn(|conn| { ... })` pattern.
5. **Embedding model token limit** -- nomic-embed-text has 8192 token limit. Always truncate to 2000 chars before embedding.

## Module Map

### lv-engine::storage
- `db.rs` -- DuckDB connection with Mutex wrapper. `with_conn()` pattern.
- `viking_fs.rs` -- Filesystem abstraction. ls/mkdir/rm/mv/read/write/vector_search over SQL.
- `embedding_queue.rs` -- Batch background worker. Collects 20 tasks, calls embed_batch(), writes to DB.
- `schema.rs` -- DuckDB CREATE TABLE statements.

### lv-engine::llm
- `embedder.rs` -- `Embedder` trait. `OpenAiEmbedder` (Ollama/vLLM) + `LocalEmbedder` (fastembed ONNX).
- `chat.rs` -- `ChatModel` trait. `OpenAiChat` + `GeminiChat` (internal proxy).
- `prompts.rs` -- L0/L1/compression/memory extraction prompt templates.

### lv-engine::parse
- `code_parser.rs` -- Tree-sitter AST parsing for C, Python, JS/TS, Rust, Go. Extracts functions/classes/structs with leading comments as individual nodes.
- `markdown.rs` -- Split by H1/H2, merge small sections, cap at 30 nodes. Fallback for non-code files.
- `pipeline.rs` -- Import pipeline: fetch URL/file/directory -> try code parser -> fallback to markdown -> store -> queue embeddings.

### lv-engine::service
- `fs_service.rs` -- Filesystem ops + triggers embedding on write.
- `search_service.rs` -- Hierarchical retrieval: global top-k -> directory walk -> expand -> rank.
- `session_service.rs` -- Create/message/commit sessions. LLM memory extraction on commit.
- `resource_service.rs` -- Wraps ImportPipeline for gRPC handler.
- `debug_service.rs` -- System status (context/session/vector/pending counts).

## Evaluation Results

Tested on 5 corpora vs upstream OpenViking:

| Corpus | LiteVikings | OpenViking |
|--------|-------------|-----------|
| DuckDB docs (383 md) | 7/10 | 8/10 |
| SQLite source (149 C/H) | 7/10 | 0/10 |
| Paperclip Ruby (62 files) | 6/10 | 5/10 |
| Paperclip AI (200 TS/MD) | avg 0.73 | avg 0.64 |
| Lightpanda (324 Zig) | avg 0.69 | avg 0.66 |

OpenViking wins on pure markdown. LiteVikings wins on source code (OpenViking fails to embed large files).

## TODOs

See `docs/bugs/002-known-issues.md` for tracked issues.

Major remaining work:
- **Separate L0/embed queues** -- vectors searchable immediately, L0 abstracts arrive later
- **Pack import/export** -- portable `.ovpack` format
- **More tree-sitter languages** -- Zig, Ruby, PHP, Kotlin, Scala, Swift
