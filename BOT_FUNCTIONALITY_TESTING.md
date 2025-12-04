# Bot Functionality Testing Guide

This guide explains how to test that the Sol Beast bot is actually working - detecting new coins and receiving transactions - not just loading without errors.

## Why This Testing is Important

The initial deployment tests (`test-with-playwright.mjs`) verify that:
- ✅ The app loads without critical errors
- ✅ WASM module initializes
- ✅ UI renders correctly

However, these tests don't verify that the bot is actually **functioning** - monitoring the blockchain, detecting new tokens, and processing transactions. This requires:
- Working RPC endpoints with CORS support
- Active monitoring of Solana blockchain
- Time to detect actual transactions (minutes, not seconds)

## Prerequisites

### 1. Working RPC Endpoints

You **MUST** have access to premium RPC providers that support CORS for browser requests:

- **Helius**: https://helius.dev
- **QuickNode**: https://quicknode.com  
- **Alchemy**: https://alchemy.com

Public Solana RPC endpoints (`https://api.mainnet-beta.solana.com`) **DO NOT** work in the browser due to CORS restrictions.

### 2. API Keys

Get your API keys from your chosen provider and construct your endpoints:

**Helius:**
```
HTTPS: https://mainnet.helius-rpc.com/?api-key=YOUR_KEY
WSS:   wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY
```

**QuickNode:**
```
HTTPS: https://your-endpoint.quiknode.pro/YOUR_KEY/
WSS:   wss://your-endpoint.quiknode.pro/YOUR_KEY/
```

**Alchemy:**
```
HTTPS: https://solana-mainnet.g.alchemy.com/v2/YOUR_KEY
WSS:   wss://solana-mainnet.g.alchemy.com/v2/YOUR_KEY
```

## Running the Bot Functionality Test

### Step 1: Build the Application

```bash
# Build WASM module
./build-wasm.sh

# Build documentation  
./build-docs.sh

# Build frontend
cd frontend
npm ci
NODE_ENV=production VITE_USE_WASM=true npm run build:frontend-webpack

# Create GitHub Pages structure
mkdir -p dist_gh/sol_beast
cp -r dist/* dist_gh/sol_beast/
mkdir -p dist_gh/sol_beast/sol_beast_docs
cp -r ../sol_beast_docs/book/* dist_gh/sol_beast/sol_beast_docs/
```

### Step 2: Start the Server

```bash
# From frontend directory
npx serve dist_gh -l 8080
```

Keep this terminal open. The app will be available at: `http://localhost:8080/sol_beast/`

### Step 3: Run the Bot Functionality Test

Open a **new terminal** and run:

```bash
# Set your RPC endpoints
export SOLANA_RPC_URL="https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
export SOLANA_WS_URL="wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY"

# Go to frontend directory (where Playwright is installed)
cd frontend

# Run the extended test (3 minutes monitoring)
node test-bot-functionality.mjs http://localhost:8080/sol_beast/ 180

# Or for longer monitoring (5 minutes)
node test-bot-functionality.mjs http://localhost:8080/sol_beast/ 300
```

### What the Test Does

1. **Opens the browser** (visible, not headless)
2. **Configures RPC endpoints** using your environment variables
3. **Starts the bot** in DRY RUN mode
4. **Monitors console logs** for:
   - "New coin detected" messages
   - "Received tx" (transaction) messages
   - Error messages
5. **Takes screenshots** at different stages:
   - `bot-test-01-initial.png` - Initial load
   - `bot-test-02-configured.png` - After RPC configuration
   - `bot-test-03-started.png` - Bot started
   - `bot-test-04-monitoring-XXs.png` - Periodic updates
   - `bot-test-05-final.png` - Final state
6. **Generates a report** (`bot-functionality-report.json`)

### Expected Output

When the bot is working correctly, you should see console output like:

```
[LOG] Bot service initialized (WASM mode)
[LOG] WebSocket connected to wss://mainnet.helius-rpc.com/...
[INFO] Monitoring started for program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
[LOG] Received tx: <signature>
[LOG] New token detected: <mint_address>
[INFO] Token metadata: {"name": "...", "symbol": "..."}
```

The test will report:
- ✅ `New Coins Detected: X` (where X > 0)
- ✅ `Transactions Received: X` (where X > 0)

### Success Criteria

The test is **SUCCESSFUL** when:

1. ✅ Bot starts without errors
2. ✅ RPC/WebSocket connection established
3. ✅ New coins are being detected (count > 0)
4. ✅ Transactions are being received (count > 0)
5. ✅ Console shows activity logs (not just static)

### Failure Scenarios

❌ **Bot doesn't start:**
- Check RPC endpoints are correct
- Verify API keys are valid
- Check console for error messages

❌ **No coins detected after 3+ minutes:**
- This might be normal if the blockchain is quiet
- Try running for longer duration (5-10 minutes)
- Check that pump.fun program is active

❌ **CORS errors:**
- Your RPC provider doesn't support browser CORS
- Switch to a different provider
- Verify API key has CORS enabled

## Manual Testing Alternative

If you want to test manually without the script:

1. **Open the app** in Chrome: `http://localhost:8080/sol_beast/`

2. **Open Developer Console** (F12 or Ctrl+Shift+I)

3. **Go to Configuration panel** and update RPC URLs

4. **Go to Dashboard** and click "START BOT"

5. **Watch the Logs panel** and browser console for:
   - New coin detection messages
   - Transaction messages
   - Error messages

6. **Wait 3-5 minutes** for activity

7. **Take screenshots** of:
   - Dashboard with bot running
   - Logs panel showing activity
   - New Coins panel showing detected tokens (if any)
   - Browser console showing transaction logs

## Testing in GitHub Actions (CI)

To test in CI with repository secrets:

1. **Add secrets** to your repository:
   - Go to Settings → Secrets and variables → Actions
   - Add `SOLANA_RPC_URL` with your HTTPS endpoint
   - Add `SOLANA_WS_URL` with your WSS endpoint

2. **Create a workflow** (`.github/workflows/test-bot-functionality.yml`):

```yaml
name: Test Bot Functionality

on:
  workflow_dispatch:
    inputs:
      duration:
        description: 'Monitor duration in seconds'
        required: false
        default: '180'

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: '20.x'
          
      - name: Setup Rust & Build WASM
        run: |
          # Install Rust, wasm-pack, etc.
          # Run build-wasm.sh
          # Build frontend
          
      - name: Run Bot Functionality Test
        env:
          SOLANA_RPC_URL: ${{ secrets.SOLANA_RPC_URL }}
          SOLANA_WS_URL: ${{ secrets.SOLANA_WS_URL }}
        run: |
          cd frontend
          npm ci
          npx playwright install chromium
          node test-bot-functionality.mjs http://localhost:8080/sol_beast/ ${{ github.event.inputs.duration }}
          
      - name: Upload Screenshots
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: bot-test-screenshots
          path: frontend/bot-test-*.png
          
      - name: Upload Report
        if: always()
        uses: actions/upload-artifact@v3
        with:
          name: bot-functionality-report
          path: frontend/bot-functionality-report.json
```

## Troubleshooting

### "Playwright not found" Error

```bash
cd frontend
npm install -D playwright@latest
npx playwright install chromium
```

### "No new coins detected"

This is normal if:
- The blockchain is quiet
- The pump.fun program has no new tokens
- You're testing during off-peak hours

Solutions:
- Run for longer duration (5-10 minutes)
- Try testing during peak hours (US evening time)
- Check that your RPC is actually connected (look for WebSocket messages)

### CORS Errors in Console

```
Access to fetch at 'https://api.mainnet-beta.solana.com' from origin 'http://localhost:8080' 
has been blocked by CORS policy
```

Solution: You're using public RPC endpoints which don't support CORS. Switch to Helius, QuickNode, or Alchemy.

### Rate Limit Errors

```
Error: 429 Too Many Requests
```

Solution: Your RPC provider's free tier has rate limits. Upgrade to paid tier or reduce request frequency.

## What to Report

When reporting bot functionality test results, include:

1. **Screenshots** showing:
   - Bot in RUNNING state
   - Logs panel with activity
   - New Coins panel with detected tokens
   - Browser console with transaction logs

2. **Console logs** showing:
   - "New coin detected" messages
   - "Received tx" messages
   - Any error messages

3. **Test report** (`bot-functionality-report.json`)

4. **Duration tested** (how many minutes)

5. **RPC provider used** (Helius/QuickNode/Alchemy)

## Example Successful Test Output

```
=== Sol Beast Bot Functionality Test ===
Target URL: http://localhost:8080/sol_beast/
Monitor Duration: 180 seconds
RPC URL: https://mainnet.helius-rpc.com/?api-key=***
WS URL: wss://mainnet.helius-rpc.com/?api-key=***

=== Step 1: Navigate to application ===
✓ App loaded
✓ Initial screenshot saved

=== Step 2: Configure RPC endpoints ===
✓ RPC configuration submitted

=== Step 3: Start the bot in DRY RUN mode ===
✓ DRY RUN mode selected
Clicking START BOT...
✅ Bot started successfully!

=== Step 4: Monitor bot activity for 180 seconds ===
[LOG] WebSocket connected
[INFO] Monitoring program 6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P
[LOG] Received tx: 4kj3h5jk...
[LOG] New token detected: 7y8h9j0k...
[INFO] Token metadata loaded: TokenMoon (TMN)

--- Status Update (30s elapsed) ---
New coins detected: 1
Transactions received: 3
Console messages: 45
Errors: 0

[LOG] Received tx: 2mn3b4v5...
[LOG] New token detected: 9p0q1r2s...

--- Status Update (60s elapsed) ---
New coins detected: 2
Transactions received: 7
Console messages: 89
Errors: 0

...

================================================================================
BOT FUNCTIONALITY TEST REPORT
================================================================================

--- RESULTS ---
Bot Started: ✅ YES
RPC Configured: ✅ YES
New Coins Detected: 5
Transactions Received: 23
Total Console Messages: 234
Errors: 0

--- BOT ACTIVITY LOGS (Last 20) ---
[1] NEW COIN: New token detected: 7y8h9j0k...
[2] TX: Received tx: 4kj3h5jk...
[3] NEW COIN: New token detected: 9p0q1r2s...
...

✅ TEST PASSED: Bot is functioning and detecting new coins
```

This confirms the bot is **actually working** - not just loading without errors, but actively monitoring and detecting blockchain activity.
