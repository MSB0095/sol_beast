# Testing GitHub Pages Deployment

This document explains how to test the Sol Beast GitHub Pages deployment locally before pushing to production.

## Overview

Sol Beast deploys to GitHub Pages as a pure WASM application running entirely in the browser. This testing infrastructure allows you to:

1. Build the exact same artifacts as the production deployment
2. Serve them locally with the same directory structure as GitHub Pages
3. Test with Playwright to detect console errors and issues
4. Generate reports and screenshots

## Quick Start

### Option 1: Automated Testing with Playwright

Run the complete test suite:

```bash
# Run the test deployment script
./test-deployment.sh
```

This will:
- Build WASM module
- Build documentation
- Build frontend with webpack
- Start a local server on port 8080
- Open the app at http://localhost:8080/sol_beast/

### Option 2: Manual Testing

```bash
# 1. Build WASM
./build-wasm.sh

# 2. Build documentation
./build-docs.sh

# 3. Install frontend dependencies (if not already done)
cd frontend
npm ci

# 4. Build frontend in production mode
NODE_ENV=production VITE_USE_WASM=true npm run build:frontend-webpack

# 5. Copy documentation
mkdir -p dist/sol_beast_docs
cp -r ../sol_beast_docs/book/* dist/sol_beast_docs/

# 6. Create GitHub Pages directory structure
mkdir -p dist_gh/sol_beast
cp -r dist/* dist_gh/sol_beast/

# 7. Serve the app
npx serve dist_gh -l 8080

# 8. Open browser to http://localhost:8080/sol_beast/
```

### Option 3: Playwright Browser Testing (Initial Load)

Run comprehensive browser testing with error detection:

```bash
# Install Playwright (one-time setup)
cd frontend
npm install -D playwright@latest
npx playwright install chromium

# Build and serve the app first (see Option 2)

# Then run the Playwright test from the root directory
cd ..
node test-with-playwright.mjs http://localhost:8080/sol_beast/
```

### Option 4: Bot Functionality Testing (Extended Monitoring)

Test the bot's actual functionality with RPC connection and transaction monitoring:

```bash
# Set RPC endpoints (use working CORS-enabled endpoints)
export SOLANA_RPC_URL="https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
export SOLANA_WS_URL="wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY"

# Run extended test (default 3 minutes monitoring)
node test-bot-functionality.mjs http://localhost:8080/sol_beast/ 180

# Or with custom duration (e.g., 5 minutes)
node test-bot-functionality.mjs http://localhost:8080/sol_beast/ 300
```

This test will:
1. Configure RPC endpoints automatically
2. Start the bot in DRY RUN mode
3. Monitor console for new coin detection and transactions
4. Take periodic screenshots
5. Generate a detailed activity report

This generates:
- `deployment-test-full.png` - Full page screenshot
- `deployment-test-viewport.png` - Viewport screenshot
- `deployment-test-report.json` - Detailed JSON report with all console messages

## GitHub Actions Workflow

### Automated Testing in CI

The `.github/workflows/test-deployment.yml` workflow can be triggered manually to test the deployment in GitHub Actions:

1. Go to your repository on GitHub
2. Click "Actions" tab
3. Select "Test GitHub Pages Deployment"
4. Click "Run workflow"
5. Wait for it to complete
6. Download the screenshot artifact to see results

### Using Repository Secrets for RPC URLs

To test with working RPC URLs instead of public endpoints:

1. Go to your repository **Settings**
2. Go to **Secrets and variables** → **Actions**
3. Add the following secrets:
   - `SOLANA_RPC_URL` - Your HTTPS RPC endpoint (e.g., from Helius, QuickNode, Alchemy)
   - `SOLANA_WS_URL` - Your WebSocket endpoint

The test workflow will automatically use these if available.

## Environment Variables

The test scripts support these environment variables:

```bash
# RPC endpoints (optional)
export SOLANA_RPC_URL="https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
export SOLANA_WS_URL="wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY"

# Then run the test
./test-deployment.sh
```

## Understanding Test Results

### Expected Console Messages

✅ **Normal/Expected:**
- "Initializing WASM module..."
- "✓ WASM bot initialized successfully"
- "Bot service initialized (WASM mode)"
- "No saved settings found, using defaults"

⚠️ **Expected in Test Environment (Not in Production):**
- External CDN failures (iconify, Google Fonts) - blocked by test environment
- Backend API 404s (health, settings) - expected before WASM mode activates

❌ **Unexpected/Critical:**
- JavaScript exceptions
- WASM initialization failures
- Missing critical assets
- UI rendering errors

### What to Look For

1. **WASM Loading:**
   - Check that WASM module loads successfully
   - Look for "WASM bot initialized successfully" message
   - Verify no WASM-related errors

2. **UI Rendering:**
   - All panels should render correctly
   - Navigation should work
   - No broken layouts or missing components

3. **Console Errors:**
   - Review all error messages
   - Distinguish between critical and non-critical errors
   - Verify no JavaScript exceptions

4. **Network Requests:**
   - Check that assets load from correct paths
   - Verify base path is `/sol_beast/`
   - Look for any 404s on critical resources

## Common Issues

### Issue: 404 on index.html or assets

**Cause:** Incorrect directory structure or base path

**Solution:**
- Ensure you're serving from `dist_gh/` directory
- The app should be accessible at `/sol_beast/` not root `/`
- Check that webpack build used correct `BASE_PATH`

### Issue: WASM module fails to load

**Cause:** Build error or incorrect MIME type

**Solution:**
- Rebuild WASM: `./build-wasm.sh`
- Check that `*.wasm` file exists in `dist/`
- Verify server serves `.wasm` with correct MIME type

### Issue: App shows "Connection Lost" banner

**Cause:** Expected behavior in WASM mode

**Solution:**
- This is normal! The app tries to connect to backend first
- It will automatically fall back to WASM mode
- The banner should disappear after a few seconds

### Issue: RPC connection errors

**Cause:** Using public endpoints that don't support CORS

**Solution:**
- Configure premium RPC endpoints (Helius, QuickNode, Alchemy)
- Set environment variables: `SOLANA_RPC_URL` and `SOLANA_WS_URL`
- Or use the RPC Configuration modal in the app

## Files Created by Testing

```
deployment-test-full.png          # Full page screenshot
deployment-test-viewport.png      # Viewport screenshot
deployment-test-report.json       # Detailed JSON report
frontend/dist/                    # Webpack build output
frontend/dist_gh/                 # GitHub Pages structure
sol_beast_docs/book/              # Built documentation
```

All of these are gitignored and won't be committed.

## Production Deployment

After testing locally and everything looks good:

1. Commit your changes
2. Push to `master` branch
3. GitHub Actions will automatically:
   - Build WASM
   - Build documentation
   - Build frontend
   - Deploy to GitHub Pages

The production deployment uses the exact same build commands as the test scripts.

## Troubleshooting

### Playwright Installation Issues

If Playwright fails to install browsers:

```bash
# Try installing system dependencies
npx playwright install-deps

# Then install browsers
npx playwright install chromium
```

### Port Already in Use

If port 8080 is already in use:

```bash
# Find process using port 8080
lsof -i :8080

# Kill it
kill -9 <PID>

# Or use a different port
npx serve dist_gh -l 8081
```

### Build Failures

If builds fail, check:

1. **Rust/Cargo:** `rustc --version` (need 1.70+)
2. **wasm-pack:** `wasm-pack --version`
3. **Node.js:** `node --version` (need 18+)
4. **npm:** `npm --version`

Install missing tools:

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install wasm-pack
cargo install wasm-pack

# Install mdbook
cargo install mdbook

# Add wasm32 target
rustup target add wasm32-unknown-unknown
```

## Additional Resources

- See `DEPLOYMENT_TEST_RESULTS.md` for the latest test report
- See `GITHUB_PAGES_SETUP.md` for deployment documentation
- See `RPC_CONFIGURATION_GUIDE.md` for RPC setup
- See `.github/workflows/test-deployment.yml` for CI configuration

## Reporting Issues

If you encounter issues during testing:

1. Save the console output
2. Save the screenshots
3. Save the JSON report
4. Create an issue with all these artifacts attached
5. Include your environment details (OS, Node version, etc.)
