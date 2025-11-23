#!/bin/bash
# Build script for WASM module

set -e

echo "Building sol_beast WASM module..."

# Build for web (generates ES modules)
wasm-pack build --target web --out-dir ../frontend/src/wasm

echo "WASM module built successfully!"
echo "Output: frontend/src/wasm/"
