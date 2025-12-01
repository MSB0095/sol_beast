#!/bin/bash
# Build WASM module for browser

set -e

echo "Building WASM module..."

# Install wasm-pack if not installed
if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build WASM package
cd "$(dirname "$0")/sol_beast_wasm"
wasm-pack build --target web --out-dir ../frontend/src/wasm --release

echo "✓ WASM module built successfully!"
echo "Output: frontend/src/wasm/"
