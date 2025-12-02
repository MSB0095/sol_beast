# GitHub Pages Deployment Guide

This guide explains how the Sol Beast bot works on GitHub Pages without requiring a backend server.

## How It Works

The bot is designed to run entirely in the user's browser using WebAssembly (WASM). No backend server is needed.

### Architecture

1. **Static Hosting**: GitHub Pages serves the static HTML, CSS, JavaScript, and WASM files
2. **WASM Bot**: The trading bot logic runs in the browser via WebAssembly
3. **Local Storage**: Settings and state are persisted in the browser's localStorage
4. **Public APIs**: The bot connects directly to public Solana RPC and WebSocket endpoints

### Settings Management

The bot uses a multi-tiered settings approach:

1. **Default Settings**: Built into the WASM binary (`sol_beast_wasm/src/lib.rs`)
2. **Static JSON**: Fallback settings in `frontend/public/bot-settings.json`
3. **Local Storage**: User customizations saved in browser localStorage

When the bot starts:
1. It checks localStorage for saved settings
2. If no saved settings exist, it loads from `bot-settings.json`
3. If that fails, it uses the built-in defaults

### Configuration Files

#### `frontend/public/bot-settings.json`

This file contains the default configuration for the bot on GitHub Pages. You can customize these values:

```json
{
  "solana_ws_urls": ["wss://api.mainnet-beta.solana.com/"],
  "solana_rpc_urls": ["https://api.mainnet-beta.solana.com/"],
  "pump_fun_program": "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
  "metadata_program": "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
  "tp_percent": 100.0,
  "sl_percent": -50.0,
  "timeout_secs": 50,
  "buy_amount": 0.001,
  "max_holded_coins": 4,
  "slippage_bps": 500,
  "min_tokens_threshold": 30000,
  "max_sol_per_token": 0.002,
  "min_liquidity_sol": 0.0,
  "max_liquidity_sol": 15.0
}
```

### Important Notes

1. **No Server Required**: The bot does not need any backend server or API
2. **Browser-Only**: All bot logic runs in the user's browser
3. **Public RPC**: Uses public Solana RPC endpoints (you can configure premium RPC services)
4. **Dry-Run by Default**: Bot starts in dry-run mode for safety
5. **Private Keys**: Private keys (if provided) stay in the browser's localStorage only

### Deployment

When deploying to GitHub Pages:

1. Build the WASM module: `bash build-wasm.sh`
2. Build the frontend: `cd frontend && npm run build`
3. Deploy the `frontend/dist` directory to GitHub Pages

The GitHub Actions workflow handles this automatically.

### Troubleshooting

#### "Failed to get bot settings: unreachable"

This error typically means:
- The WASM module panicked (usually due to bad serialization)
- Settings are corrupted in localStorage

**Solution**: Clear browser localStorage and refresh the page. The bot will load from `bot-settings.json`.

#### "No WebSocket URL configured"

This means settings didn't load properly.

**Solution**: 
1. Check that `bot-settings.json` is accessible at `<your-site-url>/bot-settings.json`
2. Clear localStorage and refresh

#### Bot won't start

Make sure:
1. Your browser supports WebAssembly
2. You have a stable internet connection
3. The Solana RPC endpoints are accessible

### Security Considerations

1. **Private Keys**: If you enter a private key, it's stored only in your browser's localStorage
2. **No Server Transmission**: Private keys are never sent to any server
3. **HTTPS**: GitHub Pages uses HTTPS, ensuring encrypted connections
4. **Dry-Run Mode**: Always test in dry-run mode before using real funds

### Customizing RPC Endpoints

To use a custom or premium RPC service:

1. Edit `frontend/public/bot-settings.json`
2. Update `solana_ws_urls` and `solana_rpc_urls`
3. Rebuild and deploy

Or configure via the UI after the bot loads.

### Development vs Production

The bot automatically detects GitHub Pages:

```typescript
// In frontend/src/services/botService.ts
const USE_WASM = import.meta.env.VITE_USE_WASM === 'true' || 
                 window.location.hostname.includes('github.io')
```

- On `*.github.io` domains: Uses WASM mode
- Locally: Can use WASM or REST API mode via environment variable

### Base Path Configuration

For GitHub Pages (which serves from a subdirectory):

```typescript
// In frontend/vite.config.ts
base: process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/'
```

This ensures assets load correctly from `/sol_beast/` on GitHub Pages.
