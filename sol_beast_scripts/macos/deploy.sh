#!/bin/bash

# Wrapper: deploy helper in new scripts folder (Linux/macOS)
set -e
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Execute the original deploy.sh while keeping relative pointers working
bash "$SCRIPT_DIR/../../sol_beast_scripts/linux/deploy.sh" "$@"
