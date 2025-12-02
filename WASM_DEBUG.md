# WASM Debugging Guide

This guide helps debug WASM mode issues in Sol Beast.

## Quick Test

1. Build the WASM module:
   ```bash
   ./build-wasm.sh
   ```

2. Serve the test page:
   ```bash
   # Simple HTTP server
   python3 -m http.server 8000
   # or
   npx serve .
   ```

3. Open http://localhost:8000/test-wasm.html in your browser

4. Open browser DevTools (F12) and watch the Console tab

5. Click "Initialize WASM" button

6. If initialization succeeds, try "Test WebSocket Connection" and "Test RPC Connection"

7. If tests pass, try "Start Bot"

## Common Issues

### Issue: "unreachable" error

**Cause**: WASM panic due to unhandled error

**Solutions**:
- Check browser console for detailed error messages
- Verify WebSocket URL is accessible (not blocked by firewall/CORS)
- Ensure RPC URL is correct and accessible
- Check if browser supports WebSockets (all modern browsers do)

### Issue: WebSocket connection fails

**Cause**: Firewall, CORS, or invalid URL

**Solutions**:
1. Check if the WebSocket URL is correct (should start with `wss://` or `ws://`)
2. Try alternative Solana RPC providers:
   - `wss://api.mainnet-beta.solana.com/` (default)
   - `wss://solana-mainnet.core.chainstack.com/<your-key>`
   - `wss://rpc.ankr.com/solana_devnet_ws` (for testing)
3. Check browser console Network tab for connection attempts
4. Verify no browser extensions are blocking WebSocket connections
5. Try in incognito/private browsing mode

### Issue: RPC connection fails

**Cause**: Firewall, CORS, rate limiting, or invalid URL

**Solutions**:
1. Verify RPC URL (should start with `https://` or `http://`)
2. Try alternative RPC endpoints:
   - `https://api.mainnet-beta.solana.com/` (default, may have rate limits)
   - `https://solana-mainnet.core.chainstack.com/<your-key>`
   - `https://rpc.ankr.com/solana` (public, rate limited)
3. Check if you need an API key for your RPC provider
4. Verify CORS is allowed (public RPCs typically allow browser requests)

### Issue: Bot starts but no transactions detected

**Cause**: Subscription not working or no pump.fun activity

**Solutions**:
1. Check logs for "Subscription confirmed" message
2. Verify pump.fun program address is correct: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`
3. Wait a bit - pump.fun may not have constant activity
4. Check WebSocket is still connected (should see heartbeat messages in logs)

## Browser Console Messages

When WASM initializes, you should see:
```
✓ WASM bot initialized
WebSocket connection established successfully
✓ Subscription confirmed with ID: <number>
```

If you see errors like:
- `Failed to create WebSocket`: Check WebSocket URL and firewall
- `Failed to lock state`: Internal error, report as bug
- `No WebSocket URL configured`: Update settings first
- `Connection refused`: Firewall or invalid URL
- `CORS error`: Try different RPC provider or use proxy

## Debugging Steps

1. **Test WASM initialization**
   - Click "Initialize WASM"
   - Should succeed immediately
   - If fails, WASM module is not built or corrupted

2. **Test RPC connection**
   - Click "Test RPC Connection"
   - Should return latest blockhash
   - If fails, check RPC URL and network

3. **Test WebSocket connection**
   - Click "Test WebSocket Connection"
   - Should confirm connection
   - If fails, check WebSocket URL and firewall

4. **Start bot**
   - Click "Start Bot"
   - Watch logs for subscription confirmation
   - Should see "✓ Subscription confirmed" within a few seconds

5. **Check browser console**
   - Look for detailed error messages
   - Network tab shows WebSocket and fetch requests
   - Console tab shows WASM logs

## Error Handling Improvements (v2)

The following improvements have been made to prevent "unreachable" errors:

1. **Mutex lock failures**: All `.unwrap()` calls on mutex locks now return proper errors
2. **WebSocket errors**: Connection failures are logged and reported to UI
3. **Subscription failures**: Handled gracefully with user-friendly error messages
4. **Type conversion**: `.as_u64().unwrap()` replaced with safe pattern matching
5. **Closure panics**: All closures use proper error handling without panics

## Known Limitations

- WASM mode cannot write to filesystem (uses localStorage instead)
- No wallet integration yet (dry-run mode only for now)
- WebSocket reconnection not implemented (restart bot if connection drops)
- Limited to browser-accessible RPC endpoints (CORS must be enabled)

## Reporting Issues

When reporting WASM issues, please include:
1. Browser name and version
2. Full error message from browser console
3. Network tab showing WebSocket/fetch requests
4. Settings you're using (RPC/WebSocket URLs)
5. Whether test-wasm.html works or not
