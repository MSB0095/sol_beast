#!/bin/bash
set -e

echo "🔨 Building Sol Beast Documentation..."

# Check if mdbook is installed
if ! command -v mdbook &> /dev/null; then
    echo "❌ mdbook not found. Installing..."
    cargo install mdbook --version 0.4.40
fi

# Build the documentation
cd sol_beast_docs
mdbook build

echo "✅ Documentation built successfully!"
echo "📖 Output: sol_beast_docs/book/"
