# GitHub Pages Setup for WASM Deployment

This document explains how to deploy Sol Beast to GitHub Pages with WASM-only mode.

## Overview

Sol Beast can run entirely in the browser using WebAssembly (WASM), making it perfect for GitHub Pages deployment. No backend server is required in this mode.

## How It Works

### Resource Loading

The application uses a base path configuration to ensure all resources load correctly on GitHub Pages:

1. **Webpack Configuration** (`frontend/webpack.config.cjs`):
   - `publicPath`: Set to `/sol_beast/` in production (matches the repository name)
   - `import.meta.env.BASE_URL`: Defined as `/sol_beast/` for production builds
   - **CopyWebpackPlugin**: Copies `public/` folder contents (including `bot-settings.json`) to `dist/`

2. **Bot Settings** (`frontend/public/bot-settings.json`):
   - Static configuration file with default settings
   - Loaded via fetch using the base path: `${basePath}bot-settings.json`
   - Falls back to built-in defaults if loading fails

3. **WASM Module** (`frontend/src/wasm/`):
   - Built by `build-wasm.sh` script
   - Imported dynamically: `import('../wasm/sol_beast_wasm')`
   - Webpack handles the WASM file loading with correct paths using `import.meta.url`

### Deployment Process

The GitHub Actions workflow (`.github/workflows/deploy.yml`) performs these steps:

1. **Checkout Code**: Get the latest code from the repository
2. **Setup Node & Rust**: Install required build tools
3. **Install wasm-pack**: Tool for building Rust → WASM
4. **Build WASM**: Run `./build-wasm.sh` to compile Rust code to WASM
5. **Install Dependencies**: Run `npm ci` in frontend directory
6. **Build Frontend**: Run `npm run build:frontend-webpack` with:
   - `NODE_ENV=production`: Enables production optimizations
   - `VITE_USE_WASM=true`: Configures the app to use WASM mode
7. **Deploy**: Upload the `dist/` folder to GitHub Pages

### WASM Mode Detection

The application automatically detects WASM mode in two ways:

```typescript
// In frontend/src/services/botService.ts
const USE_WASM = import.meta.env.VITE_USE_WASM === 'true' || 
                 window.location.hostname.endsWith('.github.io')
```

1. **Environment Variable**: `VITE_USE_WASM=true` forces WASM mode
2. **Hostname Detection**: Automatically enables WASM when deployed on `*.github.io`

## Configuration

### Required Settings

#### 1. Repository Name in Base Path

The base path must match your GitHub repository name. Both webpack and vite configs support the `BASE_PATH` environment variable:

**Webpack Config** (`frontend/webpack.config.cjs`):
```javascript
const BASE_PATH = process.env.BASE_PATH || '/sol_beast/'
// ...
publicPath: isProduction ? BASE_PATH : '/',
```

**Vite Config** (`frontend/vite.config.ts`):
```javascript
const BASE_PATH = process.env.BASE_PATH || '/sol_beast/'
// ...
base: process.env.NODE_ENV === 'production' ? BASE_PATH : '/',
```

**To deploy to a different repository**, set the `BASE_PATH` environment variable:
```bash
# For a different repository name
BASE_PATH=/my-repo/ npm run build:frontend-webpack

# Or add to GitHub Actions workflow
env:
  BASE_PATH: /my-repo/
```

Alternatively, edit the default value in both config files to match your repository name.

#### 2. GitHub Actions Workflow

The workflow triggers on pushes to the `master` branch:

```yaml
on:
  push:
    branches:
      - master
  workflow_dispatch:  # Allows manual triggering
```

### RPC Endpoints

The default `bot-settings.json` uses free public Solana RPC endpoints:

```json
{
  "solana_ws_urls": ["wss://api.mainnet-beta.solana.com"],
  "solana_rpc_urls": ["https://api.mainnet-beta.solana.com"]
}
```

⚠️ **Warning**: Free public endpoints have strict rate limits and may provide poor performance. For production use, consider:
- [Helius](https://helius.dev)
- [QuickNode](https://www.quicknode.com)
- [Alchemy](https://www.alchemy.com)
- [Triton (RPC Pool)](https://triton.one)

## Troubleshooting

### Resources Fail to Load (404 Errors)

**Symptom**: Browser console shows `Failed to load resource: net::ERR_ABORTED 404` for files like:
- `bot-settings.json`
- `sol_beast_wasm_bg.wasm`
- JavaScript chunks

**Cause**: Incorrect base path configuration.

**Solution**: Verify that the `publicPath` in webpack config matches your repository name:
```javascript
publicPath: isProduction ? '/your-repo-name/' : '/',
```

### WASM Module Fails to Initialize

**Symptom**: Browser console shows `WASM initialization failed` or `Failed to fetch`.

**Causes & Solutions**:

1. **MIME Type Issue**: 
   - GitHub Pages should serve `.wasm` files with `application/wasm` MIME type automatically
   - If not, check browser console for MIME type errors

2. **CORS Issue**:
   - GitHub Pages sets proper CORS headers automatically
   - If using a custom domain, verify CORS configuration

3. **Build Issue**:
   - Ensure WASM was built successfully in the GitHub Actions workflow
   - Check the "Build WASM" step in the workflow logs

### Settings Not Loading

**Symptom**: Bot starts with empty or invalid settings.

**Cause**: `bot-settings.json` not found or not copied to `dist/`.

**Solution**: Verify that `copy-webpack-plugin` is configured in `webpack.config.cjs`:
```javascript
new CopyPlugin({
  patterns: [
    {
      from: 'public',
      to: '.',
      globOptions: {
        ignore: ['**/index.html'],
      },
    },
  ],
}),
```

### Connection Refused Errors (localhost:8080)

**Symptom**: Browser console shows `Failed to load resource: net::ERR_CONNECTION_REFUSED` for `localhost:8080`.

**Cause**: Application is trying to connect to backend API instead of using WASM mode.

**Solutions**:

1. **Check Environment Variable**: Ensure `VITE_USE_WASM=true` is set during build
2. **Check Hostname**: The app should automatically detect GitHub Pages hostname
3. **Check botService.ts**: Verify WASM mode detection logic:
   ```typescript
   const USE_WASM = import.meta.env.VITE_USE_WASM === 'true' || 
                    window.location.hostname.includes('github.io')
   ```

## Local Testing

To test the production build locally:

```bash
# 1. Build WASM
./build-wasm.sh

# 2. Build frontend with webpack
cd frontend
NODE_ENV=production VITE_USE_WASM=true npm run build:frontend-webpack

# 3. Serve the dist folder
# Using Python
python -m http.server 8000 --directory dist

# Using Node.js (npx serve)
npx serve dist

# Using PHP
php -S localhost:8000 -t dist
```

Then open `http://localhost:8000/sol_beast/` (note the base path!).

## GitHub Pages Settings

1. Go to your repository **Settings** → **Pages**
2. **Source**: Deploy from a branch
3. **Branch**: Select `gh-pages` (created by the workflow)
4. **Folder**: `/ (root)`
5. Click **Save**

The site will be available at: `https://your-username.github.io/sol_beast/`

## Security Considerations

### Private Keys

⚠️ **NEVER commit private keys or seed phrases to the repository!**

In WASM mode:
- Private keys are managed by the connected Solana wallet (e.g., Phantom, Solflare)
- The application uses `@solana/wallet-adapter-react` for secure wallet integration
- No private keys are stored in code or localStorage

### RPC Endpoints

When using WASM mode on GitHub Pages:
- RPC/WS calls are made directly from the browser
- Your IP address is visible to the RPC provider
- Consider using a VPN or proxy for privacy
- Use premium RPC providers with authentication for better security

## Performance Tips

1. **Use Premium RPCs**: Free endpoints are heavily rate-limited
2. **Enable Browser Caching**: GitHub Pages sets cache headers automatically
3. **Monitor Network Tab**: Check for failed requests or slow responses
4. **Use Service Workers**: Consider adding PWA support for offline capability

## Further Reading

- [WebAssembly on MDN](https://developer.mozilla.org/en-US/docs/WebAssembly)
- [wasm-pack Documentation](https://rustwasm.github.io/docs/wasm-pack/)
- [GitHub Pages Documentation](https://docs.github.com/en/pages)
- [Solana Wallet Adapter](https://github.com/solana-labs/wallet-adapter)
