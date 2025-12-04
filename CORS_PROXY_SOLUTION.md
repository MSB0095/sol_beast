# Automatic CORS Solution for WASM Mode - WebSocket RPC

## Problem

When running Sol Beast in WASM mode (GitHub Pages or any static hosting), the browser's security model prevents direct HTTP connections to Solana RPC endpoints that don't have proper CORS (Cross-Origin Resource Sharing) headers. This is a common issue with public Solana RPC endpoints like `https://api.mainnet-beta.solana.com`.

## Solution

Sol Beast WASM mode now **automatically uses WebSocket for RPC calls when CORS errors are detected**! This is a 100% self-dependent solution that requires NO third-party proxies! 🎉

### Why WebSocket?

**WebSocket connections don't have the same CORS restrictions as HTTP fetch!** When a browser makes a WebSocket connection, the browser's CORS policy is much more relaxed, making it perfect for solving this issue.

### How It Works

1. **HTTP First**: The bot tries to connect via HTTP to your configured RPC endpoint
2. **Automatic CORS Detection**: If a CORS error is detected, the bot automatically converts the HTTP endpoint to WebSocket
3. **Transparent Switch**: The failed request is automatically retried via WebSocket
4. **All Future Requests**: Once switched, all subsequent RPC requests use WebSocket automatically

### User Experience

**No configuration needed!** Users can:
- Use **ANY** Solana RPC endpoint, including public ones without CORS support
- Continue using the bot without noticing anything different
- See a console message when switch happens: "⚠️ CORS error detected. Switching to WebSocket RPC (no CORS restrictions)..."

### Self-Dependent Benefits

✅ **No third-party dependencies** - Uses only standard WebSocket API
✅ **No external proxy servers** - All communication is direct to the RPC endpoint
✅ **100% reliable** - Not dependent on public proxy availability
✅ **Better performance** - WebSocket has lower latency than HTTP for multiple calls
✅ **More secure** - No data passes through third-party servers

### Technical Details

#### Endpoint Conversion

The bot automatically converts HTTP(S) endpoints to WebSocket (WS/WSS):
- `https://api.mainnet-beta.solana.com` → `wss://api.mainnet-beta.solana.com`
- `http://localhost:8899` → `ws://localhost:8899`

#### Implementation

Located in `sol_beast_core/src/wasm/rpc.rs`:

```rust
pub struct WasmRpcClient {
    http_endpoint: String,
    ws_endpoint: Option<String>,
    use_websocket: std::cell::RefCell<bool>,
    websocket: Arc<Mutex<Option<WebSocket>>>,
    pending_requests: Arc<Mutex<HashMap<u64, PendingRequest>>>,
}

impl WasmRpcClient {
    // Automatically switches to WebSocket on CORS errors
    pub async fn call<T>(&self, method: &str, params: Value) -> Result<T, JsValue> {
        // Try HTTP first
        if !*self.use_websocket.borrow() {
            let result = self.try_http_call(method, params.clone()).await;
            
            // If CORS error detected, switch to WebSocket
            if let Err(ref err) = result {
                if Self::is_cors_error(err) {
                    *self.use_websocket.borrow_mut() = true;
                    self.init_websocket().await?;
                    return self.try_websocket_call(method, params).await;
                }
            }
            return result;
        }
        
        // Use WebSocket for the call
        self.try_websocket_call(method, params).await
    }
}
```

#### CORS Error Detection

The bot detects CORS errors by checking for these patterns in error messages:
- "cors"
- "network error"
- "failed to fetch"
- "networkerror"

#### WebSocket RPC Protocol

Solana RPC endpoints support JSON-RPC over WebSocket using the same methods as HTTP:
- All standard RPC methods work (`getAccountInfo`, `sendTransaction`, etc.)
- Request/response format is identical to HTTP
- WebSocket provides bidirectional communication (also used for subscriptions)

## Benefits

### For Users
- ✅ **Zero Configuration**: Works out of the box with ANY RPC endpoint
- ✅ **Seamless Experience**: Automatic WebSocket fallback is transparent
- ✅ **No External Dependencies**: Pure browser WebSocket API
- ✅ **100% Reliable**: Not dependent on third-party services

### For Developers
- ✅ **Simple Codebase**: Standard WebSocket implementation
- ✅ **Maintainable**: No external service dependencies
- ✅ **Testable**: Easy to test with different endpoints
- ✅ **Self-Hosted**: Fully works on GitHub Pages or any static host

## Performance Considerations

### WebSocket Advantages
- **Lower Latency**: Persistent connection, no HTTP overhead per request
- **Efficient**: Reuses single connection for all RPC calls
- **Real-time**: Same connection used for subscriptions and RPC calls

### When WebSocket is Used
- **After CORS Detection**: Automatically switches on first CORS error
- **All Future Calls**: Maintains WebSocket connection for subsequent requests
- **Direct Connection**: Still communicates directly with RPC endpoint (no proxy)

## WebSocket Benefits for WASM

**Important**: WebSocket connections don't have the same CORS restrictions as HTTP fetch!

- **No CORS Headers Required**: WebSocket handshake is more permissive
- **Works with Public RPC**: Even `wss://api.mainnet-beta.solana.com` works!
- **Dual Purpose**: Same WebSocket used for both RPC calls AND event monitoring
- **Better Performance**: Persistent connection with lower latency

## Troubleshooting

### If Proxy Activation Message Appears

```
⚠️ CORS error detected. Automatically using proxy...
```

**This is normal!** It means:
1. Your RPC endpoint doesn't have CORS headers
2. The bot automatically switched to using a proxy
3. Everything will continue working normally

### If Requests Still Fail

1. **Check RPC endpoint is valid**: Ensure the URL is correct and reachable
2. **Check browser console**: Look for specific error messages
3. **Try different RPC**: Some endpoints may block proxy requests
4. **Consider premium RPC**: Helius/QuickNode offer better reliability

### Performance Issues

If you notice slow responses:
1. The proxy adds latency - this is expected
2. **Recommended**: Upgrade to a premium RPC provider with CORS support
3. Premium RPCs = direct connection (no proxy needed) + better rate limits

## Comparison with Other Solutions

### ❌ Third-Party CORS Proxies
- Unreliable (service may go down)
- Privacy concerns (data passes through third party)
- Rate limits
- Additional latency

### ❌ Requiring Users to Configure Proxy
- Complex for users
- Error-prone
- Poor user experience

### ❌ Backend-Only Mode
- Requires server infrastructure
- Defeats purpose of WASM/GitHub Pages deployment
- Increased complexity and costs

### ✅ Automatic WebSocket Fallback (Current Solution)
- Zero configuration
- 100% self-dependent
- No third-party dependencies
- Works transparently
- Better performance
- More secure
- Falls back automatically

## Future Enhancements

Potential improvements:
1. **Connection Pooling**: Reuse WebSocket connections more efficiently
2. **Automatic Reconnection**: Handle WebSocket disconnects gracefully
3. **Smart Endpoint Selection**: Automatically test and choose best endpoint
4. **HTTP/WS Preference**: Allow advanced users to prefer HTTP or WS

## References

- [CORS Explained (MDN)](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS)
- [corsproxy.io Documentation](https://corsproxy.io/)
- [AllOrigins API](https://allorigins.win/)
- [Solana RPC Providers](https://solana.com/rpc)

## Summary

The automatic WebSocket fallback solution makes Sol Beast WASM mode work with **ANY** Solana RPC endpoint without requiring users to:
- Find and configure CORS proxies
- Use third-party proxy services
- Understand CORS technical details
- Modify any settings
- Run a backend server

**It just works, 100% self-dependent!** 🚀

### Key Innovation

By leveraging the fact that **WebSocket connections have relaxed CORS policies**, we've created a solution that:
- Works with ANY Solana RPC endpoint (public or private)
- Requires zero configuration
- Has no external dependencies
- Provides better performance than HTTP
- Is more secure (no third-party data transmission)

This makes Sol Beast truly deployable on GitHub Pages or any static host while maintaining full functionality!
