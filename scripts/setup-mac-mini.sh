#!/usr/bin/env bash
set -euo pipefail

# LiteVikings Mac Mini Setup Script
# One-liner: curl -fsSL https://raw.githubusercontent.com/shuozeli/litevikings/main/scripts/setup-mac-mini.sh | bash

LITEVIKINGS_HOME="${LITEVIKINGS_HOME:-$HOME/.litevikings}"
VLLM_PORT="${VLLM_PORT:-8000}"
GRPC_PORT="${GRPC_PORT:-50051}"
HTTP_PORT="${HTTP_PORT:-1933}"
CHAT_MODEL="${LV_CHAT_MODEL:-google/gemma-3-4b-it}"
EMBED_MODEL="${LV_EMBED_MODEL:-nomic-ai/nomic-embed-text-v1.5}"
USE_LOCAL_EMBEDDINGS="${LV_LOCAL_EMBEDDINGS:-false}"

echo "============================================"
echo "  LiteVikings Mac Mini Setup"
echo "============================================"
echo ""
echo "Config:"
echo "  Home:        $LITEVIKINGS_HOME"
echo "  vLLM port:   $VLLM_PORT"
echo "  gRPC port:   $GRPC_PORT"
echo "  HTTP port:   $HTTP_PORT"
echo "  Chat model:  $CHAT_MODEL"
echo "  Embed model: $EMBED_MODEL"
echo ""

# --- Step 1: Check prerequisites ---
echo "[1/5] Checking prerequisites..."

if ! command -v python3 &>/dev/null; then
    echo "ERROR: python3 not found. Install Python 3.10+ first."
    exit 1
fi

PYTHON_VERSION=$(python3 -c 'import sys; print(f"{sys.version_info.major}.{sys.version_info.minor}")')
echo "  Python: $PYTHON_VERSION"

if ! command -v cargo &>/dev/null; then
    echo "  Rust not found. Installing..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi
echo "  Rust: $(rustc --version | awk '{print $2}')"

# --- Step 2: Create directories ---
echo "[2/5] Setting up $LITEVIKINGS_HOME..."
mkdir -p "$LITEVIKINGS_HOME/data"
mkdir -p "$LITEVIKINGS_HOME/vllm-env"

# --- Step 3: Install vLLM ---
echo "[3/5] Installing vLLM..."
if [ ! -f "$LITEVIKINGS_HOME/vllm-env/bin/vllm" ]; then
    python3 -m venv "$LITEVIKINGS_HOME/vllm-env"
    "$LITEVIKINGS_HOME/vllm-env/bin/pip" install --upgrade pip
    "$LITEVIKINGS_HOME/vllm-env/bin/pip" install vllm
    echo "  vLLM installed."
else
    echo "  vLLM already installed."
fi

# Pre-download models
echo "  Downloading chat model: $CHAT_MODEL ..."
"$LITEVIKINGS_HOME/vllm-env/bin/python3" -c "
from huggingface_hub import snapshot_download
snapshot_download('$CHAT_MODEL')
print('  Chat model ready.')
" 2>/dev/null || echo "  WARNING: Failed to download chat model. vLLM will download on first use."

if [ "$USE_LOCAL_EMBEDDINGS" = "false" ]; then
    echo "  Downloading embedding model: $EMBED_MODEL ..."
    "$LITEVIKINGS_HOME/vllm-env/bin/python3" -c "
from huggingface_hub import snapshot_download
snapshot_download('$EMBED_MODEL')
print('  Embedding model ready.')
" 2>/dev/null || echo "  WARNING: Failed to download embedding model."
fi

# --- Step 4: Install LiteVikings ---
echo "[4/5] Building LiteVikings..."
if ! command -v lv &>/dev/null; then
    # Try to build from local source if we're in the repo
    if [ -f "Cargo.toml" ] && grep -q "litevikings" Cargo.toml 2>/dev/null; then
        cargo build --release -p lv
        cp target/release/lv "$HOME/.cargo/bin/"
    else
        cargo install --git https://github.com/shuozeli/litevikings lv
    fi
    echo "  LiteVikings installed: $(lv --help | head -1)"
else
    echo "  LiteVikings already installed: $(lv --help | head -1)"
fi

# --- Step 5: Create start script ---
echo "[5/5] Creating start script..."

cat > "$LITEVIKINGS_HOME/start.sh" << 'STARTEOF'
#!/usr/bin/env bash
set -euo pipefail

LITEVIKINGS_HOME="${LITEVIKINGS_HOME:-$HOME/.litevikings}"
VLLM_PORT="${VLLM_PORT:-8000}"
GRPC_PORT="${GRPC_PORT:-50051}"
CHAT_MODEL="${LV_CHAT_MODEL:-google/gemma-3-4b-it}"

echo "Starting vLLM on :$VLLM_PORT ..."
"$LITEVIKINGS_HOME/vllm-env/bin/vllm" serve "$CHAT_MODEL" \
    --host 0.0.0.0 --port "$VLLM_PORT" \
    --device mps \
    --max-model-len 4096 &
VLLM_PID=$!

# Wait for vLLM to be ready
echo "Waiting for vLLM to start..."
for i in $(seq 1 60); do
    if curl -s "http://localhost:$VLLM_PORT/v1/models" >/dev/null 2>&1; then
        echo "vLLM ready."
        break
    fi
    sleep 2
done

echo "Starting LiteVikings server on :$GRPC_PORT ..."
lv serve \
    --grpc-addr "0.0.0.0:$GRPC_PORT" \
    --llm-base-url "http://localhost:$VLLM_PORT/v1" \
    --chat-model "$CHAT_MODEL" \
    --local-embeddings &
LV_PID=$!

echo ""
echo "============================================"
echo "  LiteVikings is running!"
echo "============================================"
echo "  gRPC:  0.0.0.0:$GRPC_PORT"
echo "  HTTP:  0.0.0.0:1933"
echo "  vLLM:  0.0.0.0:$VLLM_PORT"
echo ""
echo "  Connect: lv --server http://$(hostname):$GRPC_PORT status"
echo ""
echo "  PIDs: vLLM=$VLLM_PID, LiteVikings=$LV_PID"
echo "  Stop: kill $VLLM_PID $LV_PID"

wait
STARTEOF

chmod +x "$LITEVIKINGS_HOME/start.sh"

echo ""
echo "============================================"
echo "  Setup complete!"
echo "============================================"
echo ""
echo "  Start the server:"
echo "    $LITEVIKINGS_HOME/start.sh"
echo ""
echo "  Or manually:"
echo "    lv serve --llm-base-url http://localhost:$VLLM_PORT/v1 --local-embeddings"
echo ""
echo "  Connect from another machine:"
echo "    lv --server http://$(hostname):$GRPC_PORT status"
echo ""
