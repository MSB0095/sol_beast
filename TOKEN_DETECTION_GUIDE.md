# Token Detection Testing & Debugging Guide

## Overview

This guide provides comprehensive instructions for testing and debugging token detection in the Sol Beast WASM bot.

## Quick Start - Testing Token Detection

### Prerequisites

1. **Premium RPC Provider** (REQUIRED)
   - Helius: https://helius.dev
   - QuickNode: https://quicknode.com
   - Alchemy: https://alchemy.com
   
   ⚠️ Public Solana RPC (`api.mainnet-beta.solana.com`) does NOT support CORS for browser requests.

2. **API Keys**
   - Get your API key from your chosen provider
   - Ensure CORS is enabled for browser requests

### Running the Test

```bash
# 1. Build everything
./build-wasm.sh
./build-docs.sh

# 2. Build frontend
cd frontend
npm ci
NODE_ENV=production VITE_USE_WASM=true npm run build:frontend-webpack

# 3. Create GitHub Pages structure
mkdir -p dist_gh/sol_beast
cp -r dist/* dist_gh/sol_beast/
mkdir -p dist_gh/sol_beast/sol_beast_docs
cp -r ../sol_beast_docs/book/* dist_gh/sol_beast/sol_beast_docs/

# 4. Start server (in one terminal)
npx serve dist_gh -l 8080

# 5. Run test (in another terminal, with your RPC URLs)
export SOLANA_RPC_URL="https://mainnet.helius-rpc.com/?api-key=YOUR_KEY"
export SOLANA_WS_URL="wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY"

# Install playwright if needed
npm install -D playwright@latest
npx playwright install chromium

# Run the test (5 minutes = 300 seconds)
node test-bot-functionality.mjs http://localhost:8080/sol_beast/ 300
```

## Expected Results

### Successful Token Detection

When token detection is working, you should see:

```
=== Sol Beast Bot Functionality Test ===
Target URL: http://localhost:8080/sol_beast/
Monitor Duration: 300 seconds
RPC URL: https://mainnet.helius-rpc.com/?api-key=***
WS URL: wss://mainnet.helius-rpc.com/?api-key=***

=== Step 1: Navigate to application ===
✓ App loaded

=== Step 2: Configure RPC endpoints ===
✓ RPC configuration submitted

=== Step 3: Start the bot in DRY RUN mode ===
✅ Bot started successfully!

=== Step 4: Monitor bot activity for 300 seconds ===
[INFO] WebSocket connected
[INFO] Subscription confirmed with ID: 12345
[LOG] Received logsNotification #1
[LOG] New pump.fun transaction detected: <signature>
[INFO] Processing transaction...
[LOG] Token evaluated: TokenName (SYMBOL)

--- Status Update (30s elapsed) ---
New coins detected: 2
Transactions received: 8
Console messages: 45
Errors: 0

...

--- RESULTS ---
Bot Started: ✅ YES
RPC Configured: ✅ YES
New Coins Detected: 5+
Transactions Received: 20+
Total Console Messages: 150+
Errors: 0

✅ TEST PASSED: Bot is functioning and detecting new coins
```

### What to Look For

1. **WebSocket Connection**
   - Should see: "WebSocket connected"
   - Should see: "Subscription confirmed with ID: X"

2. **Transaction Flow**
   - "Received logsNotification #X" messages
   - "New pump.fun transaction detected" messages
   - "Token evaluated" messages with token details

3. **UI Updates**
   - Bot status shows [RUNNING]
   - Connection shows [ONLINE]
   - Logs panel shows activity
   - New Coins panel shows detected tokens

## Troubleshooting

### Problem: WebSocket Connection Fails

**Symptoms:**
```
[ERROR] WebSocket connection failed: net::ERR_NAME_NOT_RESOLVED
[ERROR] WebSocket closed: Code: 1006 (Abnormal Closure)
```

**Solutions:**

1. **Check DNS Resolution**
   ```bash
   # Test if you can resolve the RPC domain
   nslookup mainnet.helius-rpc.com
   ping mainnet.helius-rpc.com
   ```

2. **Verify RPC URL Format**
   - Must start with `wss://` for WebSocket
   - Must include API key if required
   - Example: `wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY`

3. **Check Network Restrictions**
   - Firewall blocking WebSocket connections?
   - Corporate network blocking external connections?
   - VPN interfering with connections?

4. **Try Different RPC Provider**
   - Switch from Helius to QuickNode or Alchemy
   - Some providers have better CORS support

### Problem: CORS Errors

**Symptoms:**
```
[ERROR] Access to fetch at 'https://api.mainnet-beta.solana.com' 
from origin 'http://localhost:8080' has been blocked by CORS policy
```

**Solution:**
- You're using public Solana RPC which doesn't support CORS
- **MUST** use premium provider (Helius, QuickNode, Alchemy)
- Update RPC URLs in Configuration panel

### Problem: No Tokens Detected

**Symptoms:**
```
Bot Started: ✅ YES
RPC Configured: ✅ YES
New Coins Detected: 0
Transactions Received: 0
```

**Possible Causes:**

1. **Blockchain is Quiet**
   - Pump.fun activity varies by time
   - Try during US evening hours (peak activity)
   - Run for longer duration (10-15 minutes)

2. **Wrong Program ID**
   - Pump.fun program: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`
   - Verify this hasn't changed
   - Check pump.fun documentation

3. **Subscription Not Confirmed**
   - Should see "Subscription confirmed with ID: X"
   - If missing, WebSocket might be connected but subscription failed
   - Check RPC provider supports logsSubscribe

4. **Filtering Too Aggressive**
   - Check buy heuristics in Configuration
   - Lower thresholds to see more tokens
   - Set `min_liquidity_sol: 0.0` temporarily for testing

### Problem: Bot Crashes or Panics

**Symptoms:**
```
[ERROR] RuntimeError: unreachable
[ERROR] WebAssembly.instantiate(): Compiling function failed
```

**Solution:**
- WASM panic detected
- Check browser console for full stack trace
- Look for `.unwrap()` or `.expect()` that failed
- Report issue with full error log

## Manual Testing via UI

If you prefer to test manually without the script:

1. **Open the app** in Chrome: `http://localhost:8080/sol_beast/`

2. **Open Developer Console** (F12 or Ctrl+Shift+I)

3. **Configure RPC:**
   - Click "CONFIGURATION" tab
   - Update "WebSocket URL" with your WSS endpoint
   - Update "RPC URL" with your HTTPS endpoint
   - Click "Save Settings"

4. **Start Bot:**
   - Go to "DASHBOARD" tab
   - Select "DRY RUN" mode
   - Click "START BOT"

5. **Monitor Activity:**
   - Watch browser console for log messages
   - Check "LOGS" panel for bot activity
   - Check "NEW COINS" panel for detected tokens
   - Wait 3-5 minutes for activity

6. **Verify Detection:**
   - Look for "New pump.fun transaction detected!" in logs
   - Look for "Token evaluated" messages
   - Check if tokens appear in New Coins panel

## Understanding Log Messages

### Normal Operation

```
[INFO] Starting WASM monitor for pump.fun program: 6EF8...
[INFO] WASM bot started successfully in dry-run mode
[INFO] WebSocket connected
[INFO] Subscription confirmed with ID: 123
[INFO] Received logsNotification #1
[INFO] New pump.fun transaction detected: 4kj3h5jk...
[INFO] Processing transaction...
[INFO] Token evaluated: TokenMoon (TMN)
```

### Warning Messages (Non-Critical)

```
[WARN] Failed to fetch metadata for <mint>: HTTP 404
→ Metadata might not be uploaded yet, continuing without it

[WARN] Failed to fetch bonding curve state for <mint>
→ Using fallback price estimate

[INFO] Transaction <sig> does not contain pump.fun program ID
→ Normal, not all transactions are pump.fun related
```

### Error Messages (Critical)

```
[ERROR] Failed to parse transaction <sig>: <reason>
→ Transaction format unexpected, might need code update

[ERROR] WebSocket error occurred - Type: "error"
→ Connection problem, check RPC provider

[ERROR] Failed to lock state in process_detected_signature
→ WASM panic, report this issue
```

## Performance Expectations

### Normal Activity Levels

| Time Period | Expected Tokens | Expected Transactions |
|-------------|----------------|----------------------|
| 1 minute    | 0-2            | 2-10                 |
| 5 minutes   | 2-10           | 10-50                |
| 10 minutes  | 5-20           | 25-100               |

**Note:** Activity varies significantly by:
- Time of day (US evening = peak)
- Day of week (weekends may be slower)
- Market conditions (bull market = more activity)

### Resource Usage

| Metric | Value |
|--------|-------|
| WASM Module Size | ~762 KB |
| Memory Usage | ~50-100 MB |
| CPU Usage | Low (5-10%) |
| Network Usage | ~1-5 KB/sec |

## Debugging Checklist

Before reporting issues, verify:

- [ ] WASM module builds successfully (`./build-wasm.sh`)
- [ ] Frontend builds successfully (`npm run build:frontend-webpack`)
- [ ] Server is running (`npx serve dist_gh -l 8080`)
- [ ] App loads without JavaScript errors
- [ ] Bot status shows [ACTIVE] when started
- [ ] Using premium RPC provider (not public Solana RPC)
- [ ] RPC URLs are correct (wss:// for WebSocket)
- [ ] WebSocket connection succeeds
- [ ] Subscription confirmed message appears
- [ ] Waited at least 5 minutes for activity
- [ ] Checked during peak activity hours

## Advanced Debugging

### Enable Verbose Logging

Browser console should already show all WASM logs. To see more detail:

1. Open browser DevTools (F12)
2. Go to Console tab
3. Ensure all log levels are enabled (Info, Warning, Error)
4. Filter by "sol_beast" to see only bot messages

### Inspect WebSocket Traffic

1. Open browser DevTools (F12)
2. Go to Network tab
3. Filter by "WS" (WebSocket)
4. Click on the WebSocket connection
5. View Messages tab to see:
   - Subscription request
   - Subscription confirmation
   - logsNotification messages

### Check localStorage

Bot settings are persisted in localStorage:

```javascript
// In browser console:
console.log(localStorage.getItem('sol_beast_bot_settings'));
```

### Export Test Report

The test script generates `bot-functionality-report.json` with:
- All console messages
- Error log
- Activity counts
- Timing information

## Common Issues & Solutions

### Issue: "Test hangs at 'Navigate to application'"

**Solution:**
- Check server is running: `curl http://localhost:8080/sol_beast/`
- Verify port 8080 is not blocked
- Try different port: `npx serve dist_gh -l 3000`

### Issue: "Bot starts but no logs appear"

**Solution:**
- Check browser console is open
- Verify log level is set to "All" or "Verbose"
- Click "LOGS" tab in the UI to see bot logs

### Issue: "Screenshots show wrong state"

**Solution:**
- Screenshots are taken at specific intervals
- Check timestamp to understand when captured
- Look at bot-functionality-report.json for precise timing

## Reporting Issues

When reporting token detection issues, include:

1. **Test Output:**
   ```bash
   node test-bot-functionality.mjs ... > test-output.txt 2>&1
   ```

2. **Screenshots:**
   - bot-test-03-started.png (bot started state)
   - bot-test-05-final.png (final state)

3. **Test Report:**
   - bot-functionality-report.json

4. **Environment:**
   - Operating System
   - Browser version
   - RPC provider used
   - Test duration
   - Time of day tested

5. **Console Errors:**
   - Full error messages from browser console
   - Any WASM panic stack traces

## Success Criteria

The token detection is working correctly when:

✅ Bot starts without errors  
✅ WebSocket connects successfully  
✅ Subscription confirmed  
✅ Transactions are received (count > 0)  
✅ Tokens are detected (count > 0)  
✅ Tokens appear in "New Coins" panel  
✅ Logs show processing activity  
✅ No CORS errors  
✅ No WASM panics  

## Additional Resources

- **Bot Functionality Testing Guide:** `BOT_FUNCTIONALITY_TESTING.md`
- **RPC Configuration Guide:** `RPC_CONFIGURATION_GUIDE.md`
- **Deployment Test Results:** `DEPLOYMENT_TEST_RESULTS.md`
- **WASM Token Detection Analysis:** `WASM_TOKEN_DETECTION_ANALYSIS.md`

## Support

For issues not covered in this guide:
1. Check existing GitHub issues
2. Review WASM source code in `sol_beast_wasm/src/`
3. Create new issue with test output and environment details
