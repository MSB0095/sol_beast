#!/bin/bash

# Wrapper: Start Sol Beast Frontend (Linux/macOS)
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../../sol_beast_frontend"

echo "🚀 Starting Sol Beast Frontend..."
echo "📍 http://localhost:3000"

echo "Press Ctrl+C to stop"

echo "NOTE: If you haven't built the wasm module, run: ../../sol_beast_wasm/wasm-pack-build.sh"

npm run dev
