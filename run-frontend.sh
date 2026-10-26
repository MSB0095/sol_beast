#!/bin/bash

# Sol Beast - Start Frontend Only
# Use this in a second terminal after starting the backend

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/frontend"

echo "🚀 Starting Sol Beast Frontend..."
echo "📍 http://localhost:3000"
echo "⌛ Waiting for backend on http://127.0.0.1:8081"
echo ""
echo "Press Ctrl+C to stop"
echo ""

npm run dev
