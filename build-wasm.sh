#!/bin/bash
# Build WASM module for browser

set -e

echo "Building WASM module..."

# Install wasm-pack if not installed
if ! command -v wasm-pack &> /dev/null; then
    echo "Installing wasm-pack..."
    curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# Build WASM package with memory growth and larger initial memory
cd "$(dirname "$0")/sol_beast_wasm"

# Set RUSTFLAGS for memory configuration with larger limits
# Initial: 128MB (134217728 bytes), Max: 512MB (536870912 bytes)
export RUSTFLAGS="-C link-arg=--initial-memory=134217728 -C link-arg=--max-memory=536870912"

# Build with wasm-pack
wasm-pack build --target web --out-dir ../frontend/src/wasm --release

# Optional: Run wasm-opt for further optimization (requires binaryen)
if command -v wasm-opt &> /dev/null; then
    echo "Running wasm-opt optimization..."
    wasm-opt -Oz --enable-bulk-memory ../frontend/src/wasm/sol_beast_wasm_bg.wasm -o ../frontend/src/wasm/sol_beast_wasm_bg.wasm
fi

echo "✓ WASM module built successfully!"
echo "Output: frontend/src/wasm/"
echo "Memory configuration: Initial=128MB, Max=512MB"
