# RPC Configuration Guide

## Overview

Sol Beast WASM mode (browser deployment on GitHub Pages) requires proper RPC endpoint configuration to avoid CORS (Cross-Origin Resource Sharing) issues that prevent browser-based applications from connecting to public Solana RPC endpoints.

## Why is RPC Configuration Required?

Public Solana RPC endpoints like `https://api.mainnet-beta.solana.com` and `wss://api.mainnet-beta.solana.com` do NOT support CORS headers for browser requests. This is a security measure that prevents web applications from making direct requests to these endpoints from the browser.

When running in WASM mode (GitHub Pages or any static hosting), the application runs entirely in your browser and must connect directly to RPC endpoints. Without proper CORS support, these connections will fail.

## Solution: Premium RPC Providers

To use Sol Beast in WASM mode, you must configure RPC endpoints from providers that support CORS for browser applications. The most popular options include:

### Recommended Providers

1. **Helius**
   - Website: https://helius.dev
   - HTTPS Example: `https://mainnet.helius-rpc.com/?api-key=YOUR_API_KEY`
   - WSS Example: `wss://mainnet.helius-rpc.com/?api-key=YOUR_API_KEY`
   - Free tier available with API key

2. **QuickNode**
   - Website: https://quicknode.com
   - HTTPS Example: `https://your-endpoint.quiknode.pro/YOUR_API_KEY/`
   - WSS Example: `wss://your-endpoint.quiknode.pro/YOUR_API_KEY/`
   - Free trial available

3. **Alchemy**
   - Website: https://alchemy.com
   - HTTPS Example: `https://solana-mainnet.g.alchemy.com/v2/YOUR_API_KEY`
   - WSS Example: `wss://solana-mainnet.g.alchemy.com/v2/YOUR_API_KEY`
   - Free tier available

## Configuration Process

### First-Time Setup

When you first load Sol Beast in WASM mode, you will see a **mandatory RPC Configuration modal** that blocks access to the application until you configure valid endpoints.

#### Steps:

1. **Add HTTPS RPC URL(s)**
   - Enter at least one HTTPS RPC endpoint
   - URL must start with `https://`
   - Example: `https://mainnet.helius-rpc.com/?api-key=YOUR_KEY`

2. **Add WSS WebSocket URL(s)**
   - Enter at least one WSS WebSocket endpoint
   - URL must start with `wss://`
   - Example: `wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY`

3. **Test Connection (Optional but Recommended)**
   - Click "Test Connection" to verify both endpoints work
   - Green checkmarks indicate successful connections
   - Red X marks indicate connection failures

4. **Save & Continue**
   - Once you have valid endpoints configured, click "Save & Continue"
   - Configuration is saved to browser localStorage
   - You can now access the main application

### Updating Configuration Later

You can update your RPC configuration at any time:

1. Navigate to the **Configuration** panel
2. Find the "RPC & WebSocket Configuration" section
3. Click the **"Guided Setup"** button (appears in WASM mode only)
4. Update your endpoints using the same modal
5. Test and save

## Configuration Storage

- RPC configuration is stored in browser localStorage under the key `sol_beast_user_rpc_config`
- Configuration persists across browser sessions
- Clearing browser data will reset the configuration
- Each browser/device requires separate configuration

## Multiple Endpoints

You can configure multiple RPC endpoints for redundancy:

- The application will use the first endpoint in each list
- Additional endpoints can be used for fallback (if implemented in future updates)
- Separate multiple URLs with newlines in the Configuration panel textarea

## Troubleshooting

### Connection Test Fails

**HTTPS Connection Failed:**
- Verify the URL is correct and starts with `https://`
- Check that your API key is valid
- Ensure the provider supports CORS for browser requests
- Try a different provider

**WSS Connection Failed:**
- Verify the URL is correct and starts with `wss://`
- Check that your API key is valid
- Ensure your firewall/network allows WebSocket connections
- Try a different provider

### Modal Doesn't Appear

If the RPC configuration modal doesn't appear on first load:
- Clear browser localStorage for the site
- Refresh the page
- The modal should appear automatically

### App Still Shows Connection Errors After Configuration

1. Check that you saved the configuration properly
2. Verify both HTTPS and WSS endpoints are configured
3. Try testing connections again
4. Check browser console for detailed error messages
5. Restart the bot if it was running during configuration change

## Backend Mode vs WASM Mode

**Backend Mode** (self-hosted with Rust server):
- Does NOT require this RPC configuration
- Server can connect to any RPC endpoint (no CORS restrictions)
- Uses traditional file-based configuration (`config.toml`)

**WASM Mode** (GitHub Pages/browser-only):
- REQUIRES this RPC configuration
- Subject to browser CORS policies
- Uses localStorage for configuration persistence

## Security Notes

- API keys in RPC URLs are visible in browser localStorage
- Only use API keys with appropriate rate limits and restrictions
- Never use API keys with billing access in browser applications
- Consider using provider-specific security features (IP whitelisting, referrer restrictions)

## Cost Considerations

Most providers offer:
- **Free tiers**: Limited requests per second (often sufficient for personal use)
- **Paid tiers**: Higher rate limits and premium features
- **API key required**: Sign up on provider website to get your key

Choose a tier that matches your trading volume and frequency.
