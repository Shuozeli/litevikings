# Mac Mini Deployment Guide

## Overview

The Mac Mini is the first-class deployment target for LiteVikings. This guide
sets up a complete server-client architecture:

```
Mac Mini (Apple Silicon)
+------------------------------------------+
| vLLM server (:8000)                      |
|   - gemma-3-4b-it (chat, L0/L1 gen)     |
|   - nomic-embed-text-v1.5 (embeddings)  |
+------------------------------------------+
| LiteVikings Server                       |
|   - gRPC :50051 (CLI clients)            |
|   - HTTP :1933 (Python SDK)              |
|   - DuckDB (single file)                 |
+------------------------------------------+
         |
    Private network / Tailscale / Public
         |
+------------------------------------------+
| Clients                                  |
|   - lv find "query"                      |
|   - lv ls viking://resources             |
|   - Python SDK (HTTP)                    |
+------------------------------------------+
```

## Quick Start (One-liner)

```bash
curl -fsSL https://raw.githubusercontent.com/shuozeli/litevikings/main/scripts/setup-mac-mini.sh | bash
```

Or step by step:

## Prerequisites

- macOS with Apple Silicon (M1/M2/M3/M4)
- 16GB+ RAM recommended
- Python 3.10+ (for vLLM)
- Rust toolchain (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)

## Step 1: Install vLLM

```bash
# Create a Python virtual environment
python3 -m venv ~/.litevikings/vllm-env
source ~/.litevikings/vllm-env/bin/activate

# Install vLLM
pip install vllm
```

## Step 2: Download Models

```bash
# Chat model (gemma-3-4b-it, ~3GB, fits comfortably in 16GB RAM)
# vLLM downloads on first use, but we can pre-pull:
python3 -c "from huggingface_hub import snapshot_download; snapshot_download('google/gemma-3-4b-it')"

# Embedding model (nomic-embed-text-v1.5, ~550MB)
python3 -c "from huggingface_hub import snapshot_download; snapshot_download('nomic-ai/nomic-embed-text-v1.5')"
```

## Step 3: Start vLLM

Run two vLLM instances (or one with model routing):

```bash
# Option A: Single vLLM with both models (recommended)
# vLLM supports serving multiple models on different paths
vllm serve google/gemma-3-4b-it \
    --host 0.0.0.0 --port 8000 \
    --device mps \
    --max-model-len 4096 &

# For embeddings, run a separate instance:
vllm serve nomic-ai/nomic-embed-text-v1.5 \
    --host 0.0.0.0 --port 8001 \
    --device mps \
    --task embed &

# Option B: Use local ONNX embeddings (no second vLLM needed)
# LiteVikings includes fastembed for offline embeddings.
# Pass --local-embeddings to lv serve.
```

## Step 4: Install LiteVikings

```bash
cargo install --git https://github.com/shuozeli/litevikings lv
```

Or build from source:

```bash
git clone https://github.com/shuozeli/litevikings.git
cd litevikings
cargo build --release
cp target/release/lv ~/.cargo/bin/
```

## Step 5: Start LiteVikings Server

```bash
# With vLLM for both chat and embeddings:
lv serve \
    --grpc-addr 0.0.0.0:50051 \
    --llm-base-url http://localhost:8000/v1 \
    --chat-model google/gemma-3-4b-it \
    --embed-model nomic-ai/nomic-embed-text-v1.5

# Or with local ONNX embeddings (simpler, no second vLLM):
lv serve \
    --grpc-addr 0.0.0.0:50051 \
    --llm-base-url http://localhost:8000/v1 \
    --chat-model google/gemma-3-4b-it \
    --local-embeddings
```

## Step 6: Connect Clients

### From the same machine

```bash
lv status
lv mkdir viking://resources/docs
lv ls viking://resources
```

### From another machine on the network

```bash
# Replace MAC_MINI_IP with the Mac Mini's IP address
lv --server http://MAC_MINI_IP:50051 status
lv --server http://MAC_MINI_IP:50051 find "how does auth work"
```

### Via Tailscale (recommended for remote access)

```bash
# On Mac Mini: install Tailscale
brew install tailscale
sudo tailscale up

# On client: connect using Tailscale hostname
lv --server http://mac-mini:50051 status
```

### Python SDK (HTTP gateway)

```python
# The HTTP gateway on :1933 is wire-compatible with upstream OpenViking SDK
from openviking import SyncOpenViking

client = SyncOpenViking(url="http://MAC_MINI_IP:1933")
results = client.find("transformer attention")
```

## Running as a Service (launchd)

Create `~/Library/LaunchAgents/com.litevikings.server.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.litevikings.server</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/YOU/.cargo/bin/lv</string>
        <string>serve</string>
        <string>--grpc-addr</string>
        <string>0.0.0.0:50051</string>
        <string>--local-embeddings</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>LV_LLM_BASE_URL</key>
        <string>http://localhost:8000/v1</string>
        <key>LV_CHAT_MODEL</key>
        <string>google/gemma-3-4b-it</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/litevikings.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/litevikings.err</string>
</dict>
</plist>
```

```bash
launchctl load ~/Library/LaunchAgents/com.litevikings.server.plist
```

## Model Recommendations

| Use case | Model | RAM | Notes |
|----------|-------|-----|-------|
| **Chat (default)** | gemma-3-4b-it | ~5GB | Fast, good quality for L0/L1 |
| **Chat (quality)** | gemma-3-12b-it | ~14GB | Better quality, needs 32GB Mac |
| **Embeddings (vLLM)** | nomic-embed-text-v1.5 | ~1GB | 768-dim, good retrieval |
| **Embeddings (local)** | BGE-small-en-v1.5 | ~100MB | Built-in, 384-dim, offline |

## Troubleshooting

### vLLM MPS errors

If vLLM fails with MPS errors, try:
```bash
# Force CPU fallback
PYTORCH_MPS_FALLBACK=1 vllm serve ...
```

### Port already in use

```bash
# Find and kill the process
lsof -i :50051
kill <PID>
```

### DuckDB lock error

```bash
# Only one LiteVikings server can access the DB at a time
rm ~/.litevikings/data/litevikings.duckdb.wal
```
