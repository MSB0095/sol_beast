# Sol Beast Deployment Guide

## GitHub Pages Deployment

This project is configured to deploy the frontend to GitHub Pages automatically.

### Setup

1. **Enable GitHub Pages** in your repository settings:
   - Go to Settings → Pages
   - Set Source to "GitHub Actions"

2. **Configure Repository Variables** (if needed):
   - For custom base path, update `vite.config.ts`

3. **Automatic Deployment**:
   - Every push to `main` branch triggers automatic deployment
   - Pull request merges to `main` branch trigger automatic deployment
   - Manually trigger via Actions tab → "Deploy to GitHub Pages" → "Run workflow"

### Local Development

#### Frontend

```bash
cd frontend
npm install
npm run dev
```

Frontend will run on `http://localhost:3000` and proxy API requests to `http://localhost:8080`.

#### Backend

```bash
# Copy and configure settings
cp config.example.toml config.toml
# Edit config.toml with your RPC URLs and settings

# Run in dry-run mode (default)
RUST_LOG=info cargo run

# Or run in real mode (requires wallet setup)
RUST_LOG=info cargo run --release -- --real
```

Backend API will run on `http://localhost:8080`.

## Features

### Wallet Integration

- **Connect Wallet**: Use the wallet button in the header to connect Phantom, Solflare, or other Solana wallets
- **Create Trading Wallet**: In the Profile tab, create an associated trading wallet by signing a message
- **Security**: Your trading wallet is derived from your signature and stored encrypted in browser localStorage

### Bot Control

- **Mode Switching**: Switch between Dry-Run and Real mode when bot is stopped
- **Settings Management**: Update settings only when bot is stopped to prevent issues
- **Start/Stop**: Control bot operations directly from the UI

### Configuration

- **Dynamic Settings**: Configure trading parameters, RPC endpoints, and more
- **Helius Integration**: Enable Helius Sender for ultra-low latency transactions
- **Safety Filters**: Configure min/max liquidity, slippage, and other safety parameters

## Environment Variables

### Backend

- `RUST_LOG`: Set log level (e.g., `info`, `debug`, `warn`)
- `SOL_BEAST_CONFIG_PATH`: Path to config file (default: `config.toml`)

### Frontend

- `NODE_ENV`: Set to `production` for production builds
- `VITE_API_URL`: Override API URL (optional, defaults to proxy in dev)

## Production Deployment

### Frontend (GitHub Pages)

The frontend is automatically deployed to GitHub Pages when pushing to main. It includes:

- Solana Wallet Adapter integration
- Responsive UI with cyber theme
- Real-time bot monitoring
- Settings management
- Wallet profile management

### Backend (Self-hosted)

Run the backend on your own server or VPS:

```bash
# Build release binary
cargo build --release

# Run with real mode
RUST_LOG=info ./target/release/sol_beast --real
```

**Important Security Notes:**

1. Never commit private keys or wallet files
2. Use environment variables for sensitive data
3. Configure CORS appropriately for your deployment
4. Use HTTPS for production deployments
5. Regularly update dependencies

## API Endpoints

- `GET /health` - Health check
- `GET /stats` - Bot statistics
- `GET /bot/state` - Bot running state and mode
- `POST /bot/start` - Start bot
- `POST /bot/stop` - Stop bot
- `POST /bot/mode` - Change bot mode (dry-run/real)
- `GET /settings` - Get current settings
- `POST /settings` - Update settings (bot must be stopped)
- `GET /logs` - Get bot logs
- `GET /detected-coins` - Get detected new coins
- `GET /trades` - Get trade history

## Troubleshooting

### Frontend

- **Wallet not connecting**: Check browser console for errors, ensure wallet extension is installed
- **Can't change settings**: Stop the bot first before making changes
- **Build errors**: Clear node_modules and reinstall: `rm -rf node_modules package-lock.json && npm install`

### Backend

- **Connection refused**: Ensure backend is running on port 8080
- **Transaction failures**: Check RPC URLs are valid and not rate-limited
- **Settings not saving**: Ensure config.toml exists and is writable

## License

MIT
