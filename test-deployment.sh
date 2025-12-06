#!/bin/bash
set -e

echo "=== Sol Beast GitHub Pages Deployment Test ==="
echo ""

# Check for RPC URL environment variables

echo "Using RPC URLs:"
echo "  HTTPS: $SOLANA_RPC_URL"
echo "  WSS:   $SOLANA_WS_URL"
echo ""

# Step 1: Build WASM
echo "Step 1: Building WASM..."
./build-wasm.sh

# Step 2: Build documentation
echo ""
echo "Step 2: Building documentation..."
./build-docs.sh

# Step 3: Install frontend dependencies
echo ""
echo "Step 3: Installing frontend dependencies..."
cd frontend
if [ ! -d "node_modules" ]; then
    npm ci
else
    echo "Dependencies already installed"
fi

# Step 4: Create bot-settings.json with RPC URLs
echo ""
echo "Step 4: Creating bot-settings.json with RPC URLs..."
cat > public/bot-settings.json << EOF
{
  "_comment": "Test deployment with working RPC URLs from environment variables",
  "solana_ws_urls": ["$SOLANA_WS_URL"],
  "solana_rpc_urls": ["$SOLANA_RPC_URL"],
  "pump_fun_program": "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
  "metadata_program": "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
  "tp_percent": 100.0,
  "sl_percent": -50.0,
  "timeout_secs": 50,
  "buy_amount": 0.001,
  "max_holded_coins": 4,
  "slippage_bps": 500,
  "min_tokens_threshold": 30000,
  "max_sol_per_token": 0.002,
  "min_liquidity_sol": 0.0,
  "max_liquidity_sol": 15.0,
  "dev_tip_percent": 2.0,
  "dev_tip_fixed_sol": 0.0
}
EOF

echo "Created bot-settings.json:"
cat public/bot-settings.json

# Step 5: Build frontend with webpack
echo ""
echo "Step 5: Building frontend with webpack (production mode, BASE_PATH=/ for local test)..."
# Use BASE_PATH=/ for local server so assets resolve at /assets/
BASE_PATH=/ NODE_ENV=production VITE_USE_WASM=true npm run build:frontend-webpack

# Step 6: Copy documentation to dist
echo ""
echo "Step 6: Copying documentation to dist..."
mkdir -p dist/sol_beast_docs
cp -r ../sol_beast_docs/book/* dist/sol_beast_docs/

# Step 7: Start HTTP server
echo ""
echo "Step 7: Starting HTTP server on port 8080..."
echo "The application will be available at: http://localhost:8080/sol_beast/"
echo ""
echo "Press Ctrl+C to stop the server"
echo ""

# Check if npx serve is available, otherwise use Python
if command -v npx &> /dev/null; then
    npx serve dist -l 8080
elif command -v python3 &> /dev/null; then
    cd dist
    python3 -m http.server 8080
elif command -v python &> /dev/null; then
    cd dist
    python -m SimpleHTTPServer 8080
else
    echo "Error: No HTTP server available. Please install Node.js (npx serve) or Python."
    exit 1
fi
