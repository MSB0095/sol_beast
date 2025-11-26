#!/bin/bash
# Integration tests (moved to sol_beast_scripts/linux)
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && cd ../.. && pwd)"
if [ -d "$ROOT_DIR/sol_beast_frontend" ]; then
  FRONTEND_DIR="sol_beast_frontend"
else
  FRONTEND_DIR="frontend"
fi
echo "Running quick integration tests"
echo "1. Start the frontend: cd $FRONTEND_DIR && npm run dev"
echo "2. Start the backend: cargo run --release"
echo "Checking APIs..."
curl -s http://localhost:8080/api/health | head -c 200
echo ""
curl -s http://localhost:8080/api/stats | head -c 200
echo ""
echo "Integration checks complete"
