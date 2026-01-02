#!/bin/bash
set -e

# Build script for GitHub Pages deployment
# This script builds the WASM module and the frontend, then populates the docs/ directory.

echo "Building project for GitHub Pages..."

# 1. Build WASM
echo "Step 1: Building WASM module..."
set +e
./build-wasm.sh
if [ $? -ne 0 ]; then
    echo "⚠️  WASM build failed (likely missing wasm32-unknown-unknown target)."
    echo "⚠️  Continuing with frontend build to verify deployment structure."
    echo "⚠️  NOTE: The deployed app will not function without the WASM module."
fi
set -e

# 2. Build Frontend
echo "Step 2: Building Frontend..."
cd frontend

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo "Installing frontend dependencies..."
    npm ci
fi

# Set base path for GitHub Pages.
# - Custom domain: https://example.com/             -> BASE_PATH="/" (default below)
# - Repo pages:   https://<user>.github.io/<repo>/  -> override with BASE_PATH="/<repo>/"
#   Example: BASE_PATH="/sol_beast/" ./build-gh-pages.sh
export BASE_PATH=${BASE_PATH:-"/"}
echo "Building frontend with BASE_PATH=$BASE_PATH"

# Run build
npm run build

cd ..

# 3. Prepare docs folder
echo "Step 3: Preparing docs/ directory..."
rm -rf docs
mkdir -p docs

# Copy dist content to docs
if [ -d "frontend/dist" ]; then
    cp -r frontend/dist/* docs/
    echo "Copied frontend/dist to docs/"
else
    echo "Error: frontend/dist not found!"
    exit 1
fi

# Ensure .nojekyll exists (Webpack should create it, but good to be safe)
touch docs/.nojekyll

echo "========================================"
echo "✓ Build complete! Output directory: docs/"
echo "========================================"
echo "To deploy to GitHub Pages:"
echo "1. Ensure 'docs/' is committed to your repository"
echo "2. Configure GitHub Pages settings to serve from '/docs' folder on main branch"
echo ""
