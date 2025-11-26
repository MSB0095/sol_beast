#!/bin/bash

# Start the frontend in wasm-only mode (macOS)
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [ -d "$ROOT_DIR/sol_beast_frontend" ]; then
  FRONTEND_DIR="$ROOT_DIR/sol_beast_frontend"
else
  FRONTEND_DIR="$ROOT_DIR/frontend"
fi

echo "🔧 Building WASM module for frontend (output -> $FRONTEND_DIR/src/wasm)"
bash "$ROOT_DIR/sol_beast_wasm/wasm-pack-build.sh"

echo "🚀 Starting Sol Beast Frontend (WASM mode)..."
echo "📍 http://localhost:3000"
echo "Mode: frontend-wasm"
echo "Press Ctrl+C to stop"

cd "$FRONTEND_DIR"
export VITE_RUNTIME_MODE=frontend-wasm
npm run dev
