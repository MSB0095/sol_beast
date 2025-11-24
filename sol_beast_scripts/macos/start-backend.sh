#!/bin/bash

# Wrapper: Start Sol Beast Backend (Linux/macOS)
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../../"

export RUST_LOG=info

echo "🚀 Starting Sol Beast Backend..."
echo "📍 http://localhost:8080"
echo "📊 API: http://localhost:8080/api"
echo "Press Ctrl+C to stop"

cargo run --release
