#!/bin/bash
# Build script for WASM module

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building sol_beast WASM module..."

# Build for web (generates ES modules)
if [ -z "$OUT_DIR" ]; then
	if [ -d ../sol_beast_frontend/src/wasm ] || [ -d ../sol_beast_frontend ]; then
		OUT_DIR=../sol_beast_frontend/src/wasm
	else
		OUT_DIR=../frontend/src/wasm
	fi
fi
wasm-pack build --target web --out-dir "$OUT_DIR"

echo "WASM module built successfully!"
echo "Output: $OUT_DIR"
