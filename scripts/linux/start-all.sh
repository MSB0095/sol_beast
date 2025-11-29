#!/usr/bin/env bash

# Start both backend and frontend (Linux/Mac) using wrappers
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$ROOT_DIR"

# Start backend and frontend in background so we can trap signals and shut down both
bash "$SCRIPT_DIR/start-backend.sh" &
BACKEND_PID=$!

bash "$SCRIPT_DIR/start-frontend.sh" &
FRONTEND_PID=$!

echo "Started backend (pid: $BACKEND_PID), frontend (pid: $FRONTEND_PID)"

cleanup() {
	echo "Stopping services..."
	kill -TERM "$BACKEND_PID" "$FRONTEND_PID" 2>/dev/null || true
	wait "$BACKEND_PID" 2>/dev/null || true
	wait "$FRONTEND_PID" 2>/dev/null || true
	exit 0
}

trap cleanup INT TERM

# Wait for either process to exit; if one exits, stop the other
wait -n "$BACKEND_PID" "$FRONTEND_PID"
cleanup
