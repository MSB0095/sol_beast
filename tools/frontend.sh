#!/bin/bash
# Sol Beast - Frontend Service
# Run React frontend with Vite dev server

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$SCRIPT_DIR/frontend"

echo "🚀 Starting Sol Beast Frontend..."
echo "📍 http://localhost:3000"
echo "⌛ Waiting for backend on http://localhost:8080"
echo ""
echo "Press Ctrl+C to stop"
echo ""

npm run dev
