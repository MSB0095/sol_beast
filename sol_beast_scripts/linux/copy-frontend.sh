#!/bin/bash

# Copy frontend folder to new sol_beast_frontend folder for migration/testing
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../"

if [ ! -d "$ROOT_DIR/frontend" ]; then
    echo "No 'frontend' folder found from $ROOT_DIR"
    exit 1
fi

echo "Copying frontend -> sol_beast_frontend"
rm -rf "$ROOT_DIR/sol_beast_frontend"
rsync -a --exclude='node_modules' --exclude='dist' "$ROOT_DIR/frontend/" "$ROOT_DIR/sol_beast_frontend/"

echo "Copy complete"
