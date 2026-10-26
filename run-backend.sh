#!/bin/bash

# Sol Beast - Start Backend Only
# Use this in the first terminal

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export RUST_LOG=info

echo "🚀 Starting Sol Beast Backend..."
echo "📍 http://127.0.0.1:8081"
echo "📊 API: http://127.0.0.1:8081/v1"
echo ""
echo "Press Ctrl+C to stop"
echo ""

cargo run --release -p beast-cli -- run
