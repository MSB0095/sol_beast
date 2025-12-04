# Testing GitHub Pages Deployment with WASM

This document explains how to test the GitHub Pages deployment of Sol Beast in WASM mode with Helius RPC endpoints.

## Overview

The workflow `.github/workflows/test-gh-pages.yml` has been created to:

1. Build the WASM module
2. Configure `bot-settings.json` with Helius RPC endpoints from GitHub secrets
3. Build and deploy the frontend to GitHub Pages
4. Test the deployment with the bot in **dry-run mode**

## Prerequisites

The following secrets must be configured in the GitHub repository:

- `HELIUS_HTTPS` - Helius HTTPS RPC endpoint (e.g., `https://mainnet.helius-rpc.com/?api-key=YOUR_KEY`)
- `HELIUS_WSS` - Helius WebSocket endpoint (e.g., `wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY`)

## Workflow Trigger

The workflow is triggered by:
- Pushes to the `copilot/test-gh-page-deployment` branch
- Pull requests to the `master` branch
- Manual workflow dispatch

## Testing Instructions

### 1. Trigger the Workflow

The workflow will run automatically when you push to the test branch. You can also trigger it manually from the Actions tab.

### 2. Approve the Deployment

Since the workflow deploys to the `github-pages` environment, it requires approval:

1. Go to the Actions tab in GitHub
2. Find the "Test GitHub Pages Deployment" workflow run
3. Click "Review deployments"
4. Select the `github-pages` environment
5. Click "Approve and deploy"

### 3. Access the Deployment

Once deployed, the site will be available at:
- **URL**: `https://MSB0095.github.io/sol_beast/`

### 4. Test the Bot

1. **Open Browser Console**:
   - Press `F12` or right-click → Inspect
   - Navigate to the "Console" tab
   - Keep it open during testing

2. **Configure RPC (if needed)**:
   - The bot should already have RPC URLs from the workflow
   - Verify in the Settings panel that URLs are configured
   - They should match the Helius endpoints

3. **Verify Dry Run Mode**:
   - Check that the bot is in "Dry Run" mode (shown in the UI)
   - In dry-run mode, no real transactions will be submitted

4. **Start the Bot**:
   - Click the "Start" button
   - Monitor the browser console for messages

5. **Check for Issues**:

   **❌ Common Errors to Look For**:
   - WebSocket connection failures
   - CORS errors on RPC calls
   - WASM initialization errors
   - No coins detected after several minutes
   - JavaScript errors or stack traces

   **✅ Success Indicators**:
   - `✓ WASM bot initialized successfully`
   - `✓ Bot started successfully`
   - WebSocket connection established
   - Logs appearing in the Logs panel
   - New coins detected in the New Coins panel

6. **Monitor for 2-5 Minutes**:
   - Let the bot run to detect pump.fun activity
   - Check if new tokens appear
   - Verify logs are being generated
   - Ensure no JavaScript errors

7. **Test Transaction Building (Dry Run)**:
   - If coins are detected, click "Buy" on a token
   - Verify transaction is built but NOT submitted
   - Check console for dry-run messages

## Expected Behavior

### Bot Initialization
- WASM module loads without errors
- Settings are loaded from `bot-settings.json`
- Bot defaults to "dry-run" mode
- RPC and WebSocket connections are established

### During Operation
- Bot monitors pump.fun program for new token creations
- New tokens appear in the "New Coins" panel
- Logs show detected signatures and evaluation results
- No actual transactions are submitted (dry-run mode)

### In Browser Console
You should see:
```
Initializing WASM module...
✓ WASM bot initialized successfully
✓ Loaded default settings from bot-settings.json
Bot service initialized (WASM mode)
✓ Bot started successfully
```

## Troubleshooting

### Issue: Workflow Requires Approval

**Symptom**: Workflow shows "action_required" status

**Solution**: This is expected for deployments to the `github-pages` environment. Approve the deployment as described above.

### Issue: Secrets Not Configured

**Symptom**: bot-settings.json has empty or undefined URLs

**Solution**: Ensure `HELIUS_HTTPS` and `HELIUS_WSS` secrets are set in repository settings → Secrets and variables → Actions

### Issue: WebSocket Connection Fails

**Symptom**: Console shows "WebSocket connection failed"

**Possible Causes**:
- Invalid WebSocket URL in secret
- Helius API key expired or invalid
- CORS issues with the WebSocket endpoint

**Solution**: Verify the `HELIUS_WSS` secret is correct and includes a valid API key

### Issue: No Coins Detected

**Symptom**: Bot running but no coins appear after 5+ minutes

**Possible Causes**:
- Low trading activity on pump.fun
- WebSocket subscription not working
- Program ID mismatch

**Solution**: 
- Wait longer (pump.fun activity varies)
- Check console for WebSocket messages
- Verify program ID is correct: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`

### Issue: WASM Initialization Failed

**Symptom**: "WASM initialization failed" error in console

**Solution**: 
- Check browser console for detailed error
- Clear browser cache and reload
- Ensure WASM files are properly deployed

## Reporting Results

When reporting test results, please include:

1. **Browser & Version**: (e.g., Chrome 120.0.6099.109)
2. **Deployment URL**: The GitHub Pages URL tested
3. **Console Logs**: Relevant success/error messages
4. **Screenshots**: Browser console and UI state
5. **Test Duration**: How long the bot was monitored
6. **Coins Detected**: Number of tokens found (if any)
7. **Overall Status**: ✅ PASS or ❌ FAIL with description

## Success Criteria

The test is considered successful if:

- ✅ WASM module loads without errors
- ✅ Bot initializes with Helius RPC endpoints
- ✅ WebSocket connection is established
- ✅ Bot operates in dry-run mode (no real txs)
- ✅ New coins are detected (if trading activity exists)
- ✅ Logs are generated and visible in UI
- ✅ No critical JavaScript errors in console
- ✅ Transaction building works (when clicking Buy)

## Additional Resources

- [GITHUB_PAGES_SETUP.md](./GITHUB_PAGES_SETUP.md) - General GH Pages setup guide
- [RPC_CONFIGURATION_GUIDE.md](./RPC_CONFIGURATION_GUIDE.md) - RPC configuration details
- [WASM_FEATURES.md](./WASM_FEATURES.md) - WASM mode features

## Notes

- The bot runs entirely in the browser (no backend server)
- All processing is done using WebAssembly
- In dry-run mode, transactions are built but not submitted
- The deployment uses the same code as production, just with test RPC endpoints
