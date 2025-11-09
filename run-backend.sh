#!/bin/bash

# Sol Beast - Start Backend Only
# Use this in the first terminal

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export RUST_LOG=info

echo "🚀 Starting Sol Beast Backend..."
echo "📍 http://localhost:8080"
echo "📊 API: http://localhost:8080/api"
echo ""
echo "Press Ctrl+C to stop"
echo ""

cargo run --release
