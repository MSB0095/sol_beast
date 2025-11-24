#!/bin/bash

# Start both backend and frontend (Linux/Mac) using wrappers
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Start backend in background
bash "$SCRIPT_DIR/start-backend.sh" &
PID=$!

echo "Started backend (pid: $PID)"

# Start frontend in foreground
bash "$SCRIPT_DIR/start-frontend.sh"
