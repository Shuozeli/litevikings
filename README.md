# LiteVikings

Drop-in Rust replacement for [OpenViking](https://github.com/volcengine/OpenViking) -- the Context Database for AI Agents.

Single binary. `cargo install`. No Go, Python, C++, or CMake required.

## What It Does

LiteVikings organizes an AI agent's context (memories, resources, skills) into a virtual filesystem with semantic search.

```
viking://user/alice/memories/preferences/code-style
viking://resources/docs/architecture
viking://agent/coding-bot/skills/web-search
```

Each node carries three tiers of content:
- **L0 Abstract** (~100 tokens) -- ultra-concise summary for vector search
- **L1 Overview** (~1k tokens) -- navigation-oriented summary
- **L2 Content** -- full text, loaded on demand

## Quick Start

### Server (Mac Mini recommended)

```bash
# Install
cargo install --git https://github.com/shuozeli/litevikings lv

# Install Ollama for LLM (chat + embeddings)
brew install ollama
ollama pull qwen2.5:3b
ollama pull nomic-embed-text

# Start server
lv serve --llm-base-url http://localhost:11434/v1 \
         --chat-model qwen2.5:3b \
         --embed-model nomic-embed-text \
         --embed-dim 768
```

### Client (any machine)

```bash
# Connect to server
lv --server http://mac-mini:50051 status

# Import a codebase
lv add-resource ./my-project/README.md --to viking://resources/my-project

# Semantic search
lv find "how does authentication work"

# Session management (agent memory)
lv session create
lv session message <id> --role user "I prefer Rust for systems work"
lv session commit <id>  # extracts memories via LLM
```

## Architecture

```
Client (lv CLI)  --gRPC-->  Server (lv-server)  -->  Engine  -->  DuckDB
                                                       |
                                                    Ollama / vLLM
                                                  (chat + embeddings)
```

- **5 crates**: lv-core, lv-proto, lv-engine, lv-server, lv
- **Single DuckDB file** for all data (contexts, vectors, content, sessions)
- **gRPC primary protocol** (tonic) with 6 services
- **Hierarchical retrieval**: directory-aware search, not just flat vector lookup
- **LLM-powered**: L0/L1 generation, session compression, memory extraction

## Features

| Feature | Status |
|---------|--------|
| Viking URI filesystem (ls, mkdir, rm, mv, read, write) | Done |
| Resource import (markdown, URL, local file) | Done |
| Semantic search with hierarchical directory walk | Done |
| Session management with memory extraction | Done |
| Embedding via OpenAI-compatible API or local ONNX | Done |
| L0/L1 abstract generation via LLM | Done |
| gRPC server with 6 services | Done |
| CLI with 11 commands | Done |
| HNSW vector index (DuckDB vss) | Done |
| HTTP gateway (Python SDK compat) | TODO |
| Tree-sitter AST code parsing (C, Python, JS/TS, Rust, Go) | Done |
| Pack import/export | TODO |

## Evaluation vs OpenViking

Tested on 5 corpora with 10 queries each. Same embedding model (nomic-embed-text 768-dim via Ollama) for both systems.

| Corpus | Language | Files | LiteVikings | OpenViking |
|--------|----------|-------|-------------|-----------|
| DuckDB docs | Markdown | 383 | 7/10 | **8/10** |
| SQLite source | C/H | 149 | **7/10** | 0/10 |
| Paperclip (thoughtbot) | Ruby | 62 | **6/10** | 5/10 |
| Paperclip AI | TypeScript | 200 | **avg 0.73** | avg 0.64 |
| Lightpanda Browser | Zig | 324 | **avg 0.69** | avg 0.66 |

**OpenViking** edges LiteVikings on pure markdown documentation (mature heading-based parser).

**LiteVikings** wins on source code across all 4 code corpora. OpenViking fails to embed large files (C, Ruby) and has no tree-sitter support for Ruby or Zig. LiteVikings' chunk-and-truncate approach with LLM-generated abstracts works reliably on all file types.

## LLM Backends

LiteVikings uses OpenAI-compatible API for both chat and embeddings. Works with:

- **Ollama** (recommended for Mac Mini) -- `ollama pull qwen2.5:3b`
- **vLLM** -- `vllm serve model-name --port 8000`
- **OpenAI** -- set `--llm-base-url https://api.openai.com/v1`
- **Local ONNX** (embeddings only) -- `--local-embeddings` flag, no server needed

## Mac Mini Deployment

See [docs/deploy/mac-mini.md](docs/deploy/mac-mini.md) for a complete guide including:
- One-liner setup script
- Ollama model selection
- launchd service configuration
- Tailscale remote access

## Building from Source

```bash
# Prerequisites: Rust toolchain + protoc
sudo apt install protobuf-compiler  # or: brew install protobuf

# Build
git clone https://github.com/shuozeli/litevikings.git
cd litevikings
cargo build --release

# Run tests
cargo test --workspace
```

## License

MIT
