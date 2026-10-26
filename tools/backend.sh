#!/bin/bash
# Sol Beast - Backend Service
# Run cargo backend in release mode with logging

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SCRIPT_DIR"

# Source cargo env if available (needed after fresh install)
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"

export RUST_LOG=info

echo "🚀 Starting Sol Beast Backend..."
echo "📍 http://127.0.0.1:8081"
echo "📊 API: http://127.0.0.1:8081/v1"
echo ""
echo "Press Ctrl+C to stop"
echo ""

cargo run --release -p beast-cli -- run
