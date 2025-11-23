#!/bin/zsh
# Local script to mimic GitHub Pages deployment for Vite frontend
set -e


# Build the project with base set to '/'
VITE_BASE=/ npm run build

# Remove previous local gh-pages directory if exists
rm -rf ./gh-pages

# Copy dist output to gh-pages directory
cp -r ./dist ./gh-pages

echo "Local GitHub Pages deployment complete."
echo "You can now serve ./gh-pages locally to test."

# Optional: Serve the build locally (uncomment if you want to use npx serve)
# npx serve ./gh-pages
