# Bug: Server hangs on startup due to DuckDB vss INSTALL

## Symptom

`lv serve` starts but never binds the gRPC port (50051). The process runs indefinitely with zero output and no error. `lv status` returns "Connection refused".

## Root Cause

In `engine.rs:57`, `db.create_hnsw_index()` calls `INSTALL vss; LOAD vss;` which triggers DuckDB to download the `vss` extension binary (~50MB) from `extensions.duckdb.org`. This is a blocking HTTP download inside `Database::with_conn()`, which holds a Mutex lock on the DuckDB connection.

Two problems:
1. **Blocking in async context**: The synchronous download blocks the tokio async runtime, preventing `Server::builder().serve()` from ever executing.
2. **Connection lock**: Even if wrapped in `spawn_blocking`, the DuckDB connection Mutex is held during the download, blocking all other queries (including `lv status`).

## Affected Code

- `crates/lv-engine/src/storage/db.rs:65-99` -- `create_hnsw_index()`
- `crates/lv-engine/src/engine.rs:56-57` -- called during `Engine::new()`

## Attempted Fixes

1. **Wrap in spawn_blocking + await**: Server still doesn't bind because `Engine::new` awaits the blocking task before returning.
2. **Fire-and-forget spawn_blocking (no await)**: Server binds immediately at 2s, but the background thread holds the DuckDB connection lock, making all queries hang until the download finishes.

## Proposed Fix

Option A: Skip `INSTALL vss` at startup. Pre-install the extension via a separate CLI command (`lv setup`) or require the user to run `duckdb -c "INSTALL vss"` once.

Option B: Use a separate DuckDB connection for the INSTALL command so it doesn't lock the main connection pool. DuckDB extensions are installed per-database-directory, so installing on any connection makes it available to all.

Option C: Download the vss extension binary manually and place it in DuckDB's extension directory (`~/.duckdb/extensions/`), then only `LOAD vss` (no INSTALL needed, no network call).

## Environment

- Platform: Linux x86_64 (Ubuntu 24.04)
- DuckDB: bundled via duckdb-rs crate
- Network: extension download from extensions.duckdb.org can be slow (>60s)
- LLM backend: Ollama at chenxiyuans-mac-mini-2:11434 (works fine)

## Workaround

Pre-install the vss extension by running DuckDB CLI once:
```bash
duckdb ~/.litevikings/data/litevikings.duckdb -c "INSTALL vss;"
```
Then `lv serve` will only need `LOAD vss` which is instant.
