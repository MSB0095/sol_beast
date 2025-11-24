#!/bin/bash

# Sol Beast - Start Frontend Only
# Use this in a second terminal after starting the backend

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# detect new or old frontend folder
if [ -d "$SCRIPT_DIR/sol_beast_frontend" ]; then
	FRONTEND_DIR="$SCRIPT_DIR/sol_beast_frontend"
else
	FRONTEND_DIR="$SCRIPT_DIR/frontend"
fi
cd "$FRONTEND_DIR"

echo "🚀 Starting Sol Beast Frontend..."
echo "📍 http://localhost:3000"
echo "⌛ Waiting for backend on http://localhost:8080"
echo ""
echo "Press Ctrl+C to stop"
echo ""

npm run dev
