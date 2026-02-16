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

# Wait for the backend API to be reachable before starting Vite.
# This prevents ECONNREFUSED proxy errors during backend compilation.
waited=0
max_wait=300
while [ $waited -lt $max_wait ]; do
    if curl -s --max-time 2 http://localhost:8080/api/health >/dev/null 2>&1; then
        echo "✓ Backend is ready"
        break
    fi
    sleep 3
    waited=$((waited + 3))
done

if [ $waited -ge $max_wait ]; then
    echo "⚠ Backend not detected after ${max_wait}s — starting frontend anyway"
fi

echo "Press Ctrl+C to stop"
echo ""

npm run dev
