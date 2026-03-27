# Known Issues and Improvements

Tracked from deployment and evaluation sessions. Address one by one.

## Bugs

### 002: No logging on `lv serve`
`lv serve` produces zero output -- tracing subscriber not initialized unless `RUST_LOG` env var is set. Operators can't see what's happening.

**Fix:** Initialize `tracing_subscriber::fmt::init()` in the serve command with a sensible default (info level). Currently the subscriber is in `main.rs` but uses `warn` as default, and the serve command returns before the main match block where logging would show.

**Files:** `crates/lv/src/main.rs`, `crates/lv/src/commands/serve.rs`

---

### 003: CLI arg parsing breaks on content with dashes
`lv write viking://path "content with --dashes"` fails because clap interprets `--` in content as flags.

**Fix:** Accept content via stdin pipe or `--stdin` flag. Add `--` separator support.

**Files:** `crates/lv/src/commands/write.rs`

---

## Performance

### 004: Embedding queue is serial
1,243 files at ~3s each = ~1 hour. The embedding queue processes one text at a time.

**Fix:** Batch the embedding API calls. Ollama's `/v1/embeddings` accepts arrays. Send 10-20 texts per request instead of 1. The `Embedder::embed_batch()` trait method already exists but the queue doesn't use it.

**Files:** `crates/lv-engine/src/storage/embedding_queue.rs`

---

### 005: No bulk import
`add-resource` does one file at a time over gRPC. Importing a directory requires a shell loop.

**Fix:** Add `add-resource --recursive ./dir` that walks a directory tree server-side and batches writes. The `ImportPipeline` already handles single files; extend it to walk directories.

**Files:** `crates/lv-engine/src/parse/pipeline.rs`, `crates/lv/src/commands/add_resource.rs`

---

### 006: L0/L1 generation blocks embedding queue
The background worker does both embedding + LLM abstraction generation sequentially. A slow LLM call (3-5s for L0) blocks the fast embedding call (~100ms).

**Fix:** Separate into two queues:
- **Embedding queue**: fast, batch-capable, processes in <1s per batch
- **L0/L1 queue**: slow, serial, processes in 3-5s per item

Vectors become searchable immediately after embedding. L0 abstracts arrive later and update the context.

**Files:** `crates/lv-engine/src/storage/embedding_queue.rs`, `crates/lv-engine/src/engine.rs`

---

## Features

### 007: Incremental re-index
No way to detect if content changed since last index. Re-importing the same file creates duplicate contexts.

**Fix:** Store a content hash (xxhash or sha256) per URI. On `write` or `add-resource`, compute hash and skip if unchanged. Add a `content_hash` column to the `contexts` table.

**Files:** `crates/lv-engine/src/storage/schema.rs`, `crates/lv-engine/src/storage/viking_fs.rs`

---

### 008: Stdin support for write
`lv write` takes content as a CLI argument, which breaks on large files or content with special characters.

**Fix:** Add `--stdin` flag: `cat file.md | lv write viking://path --stdin`. Read from stdin instead of argument.

**Files:** `crates/lv/src/commands/write.rs`

---

### 009: Status should show queue depth
`lv status` only shows total contexts/sessions/vectors. No visibility into pending work.

**Fix:** Add `pending_embeddings` and `pending_l0` counts to the status response. The embedding queue already tracks pending via `rx.len()`. Expose through the gRPC AdminService.

**Files:** `crates/lv-engine/src/service/debug_service.rs`, `crates/lv-proto/proto/litevikings/v1/admin.proto`

---

## Priority Order

| # | Issue | Effort | Impact |
|---|-------|--------|--------|
| 004 | Batch embedding | Small | High -- 10x faster import |
| 006 | Separate L0 and embed queues | Medium | High -- vectors available immediately |
| 002 | Logging on serve | Small | Medium -- operator visibility |
| 003 | CLI dash parsing | Small | Medium -- usability |
| 008 | Stdin for write | Small | Medium -- usability |
| 005 | Bulk recursive import | Medium | Medium -- convenience |
| 009 | Queue depth in status | Small | Low -- observability |
| 007 | Incremental re-index | Medium | Low -- needed for watch mode |
