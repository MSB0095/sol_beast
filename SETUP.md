# Sol Beast Setup Guide

This file has been moved to `sol_beast_docs/SETUP.md`. The full setup guide is available there. The original content is preserved for backwards compatibility.

## Prerequisites

### For Both Modes
- Rust 1.70+ (`rustup update`)
- Git

### For Browser Mode
- Node.js 18+ and npm
- Modern web browser
- Solana wallet extension (Phantom, Solflare, etc.)

### For CLI Mode
- Solana CLI tools (optional, for keypair generation)
- Linux/Mac (Windows via WSL2)

## Quick Start - Browser Mode

### 1. Install Dependencies

```bash
# Install wasm-pack for building WASM modules
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Verify installation
wasm-pack --version
```

### 2. Build WASM Module

```bash
cd sol_beast_wasm
./wasm-pack-build.sh
# Or manually:
# wasm-pack build --target web --out-dir ../frontend/src/wasm
```

### 3. Setup Frontend

```bash
cd ../frontend
npm install
npm run dev
```

### 4. Access the Application

1. Open browser to `http://localhost:5173`
2. Install a Solana wallet extension if you haven't already:
   - [Phantom](https://phantom.app/)
   - [Solflare](https://solflare.com/)
3. Click "Connect Wallet" in the UI
4. Approve the connection in your wallet
5. Configure your trading settings
6. Start trading!

### Browser Mode Features
- ✅ Fully decentralized - no server needed
- ✅ Your keys, your funds - wallet stays in your control
- ✅ Settings saved per wallet address
- ✅ Trade history persists locally
- ✅ Cross-platform (works anywhere)

## Quick Start - CLI Mode

### 1. Generate or Import Wallet

**Option A: Generate new wallet**
```bash
solana-keygen new --outfile keypair.json
```

**Option B: Use existing wallet**
```bash
# Copy your keypair.json to the project directory
cp ~/.config/solana/id.json ./keypair.json
```

**Option C: Use base64 environment variable (recommended)**
```bash
# Linux (GNU coreutils)
export SOL_BEAST_KEYPAIR_B64=$(cat keypair.json | base64 -w0)

# macOS (BSD base64)
export SOL_BEAST_KEYPAIR_B64=$(cat keypair.json | base64)
```

### 2. Configure Settings

```bash
cp config.example.toml config.toml
nano config.toml  # Edit your settings
```

Key settings to configure:
```toml
# Wallet (choose one method)
wallet_keypair_path = "./keypair.json"
# OR
# wallet_private_key_string = "your_base58_key"

# Trading strategy
tp_percent = 30.0        # Take profit at +30%
sl_percent = -20.0       # Stop loss at -20%
buy_amount = 0.1         # Buy 0.1 SOL worth

# RPC endpoints
solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]
solana_ws_urls = ["wss://api.mainnet-beta.solana.com"]
```

### 3. Test Configuration (Dry Run)

```bash
# Build the CLI
cargo build --release -p sol_beast_cli

# Run in dry-run mode (no real transactions)
RUST_LOG=info cargo run --release -p sol_beast_cli
```

This will:
- Connect to Solana RPC/WebSocket
- Monitor pump.fun token launches
- Simulate buy/sell without spending SOL
- Log all activities

### 4. Run in Real Mode

⚠️ **WARNING**: This will spend real SOL! Make sure you've tested thoroughly.

```bash
RUST_LOG=info cargo run --release -p sol_beast_cli -- --real
```

### 5. Monitor via Web Dashboard

The CLI mode includes a REST API and web dashboard:

```bash
# API runs on http://localhost:8080
# Open the frontend in another terminal:
cd frontend
npm run dev
# Visit http://localhost:5173
```

## Advanced Configuration

### Helius Sender (Ultra-Low Latency)

For competitive trading, enable Helius Sender:

```toml
helius_sender_enabled = true
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
helius_min_tip_sol = 0.001
helius_priority_fee_multiplier = 1.2
```

### Multiple RPC Endpoints

For reliability, configure multiple endpoints:

```toml
solana_rpc_urls = [
    "https://api.mainnet-beta.solana.com",
    "https://solana-api.projectserum.com",
    "https://rpc.ankr.com/solana"
]
```

### Safer Sniping Filters

Reduce risk with stricter filters:

```toml
enable_safer_sniping = true
min_tokens_threshold = 1000000      # Minimum tokens per buy
max_sol_per_token = 0.0001         # Maximum price per token
min_liquidity_sol = 0.01           # Minimum pool liquidity
max_liquidity_sol = 100.0          # Maximum pool liquidity
```

## Troubleshooting

### WASM Build Issues

**Problem**: `wasm-pack` not found
```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
```

**Problem**: Build fails with Rust version error
```bash
rustup update
rustup target add wasm32-unknown-unknown
```

### CLI Issues

**Problem**: "No wallet keypair configured"
```bash
# Make sure one of these is set:
# - wallet_keypair_path in config.toml
# - wallet_private_key_string in config.toml
# - SOL_BEAST_KEYPAIR_B64 environment variable
```

**Problem**: Connection timeouts
```bash
# Try different RPC endpoints
# Check your internet connection
# Verify RPC endpoint is not rate-limiting you
```

### Frontend Issues

**Problem**: Wallet won't connect
```bash
# Make sure you have a Solana wallet extension installed
# Try refreshing the page
# Check browser console for errors
```

**Problem**: WASM module not loading
```bash
# Rebuild WASM
cd sol_beast_wasm
./wasm-pack-build.sh

# Clear browser cache
# Hard refresh (Ctrl+Shift+R or Cmd+Shift+R)
```

## Development Workflow

### Iterating on Core Library

```bash
# Test core library
cargo test -p sol_beast_core

# Test with WASM target
cd sol_beast_core
cargo build --target wasm32-unknown-unknown --features wasm
```

### Frontend Development

```bash
cd frontend

# Development server with hot reload
npm run dev

# Type checking
npm run build

# Linting
npm run lint
```

### Building for Production

**WASM + Frontend:**
```bash
# Build WASM
cd sol_beast_wasm
wasm-pack build --target web --out-dir ../frontend/src/wasm

# Build frontend
cd ../frontend
npm run build

# Deploy frontend/dist to your host
```

**CLI Binary:**
```bash
cargo build --release -p sol_beast_cli
# Binary: target/release/sol_beast

# Copy to server
scp target/release/sol_beast user@server:/opt/sol_beast/
```

## Environment Variables

### Browser Mode
```bash
# Optional: Custom Solana RPC
export VITE_SOLANA_RPC_URL="https://your-rpc-endpoint.com"
```

### CLI Mode
```bash
# Recommended: Keypair as base64
# Linux (GNU coreutils)
export SOL_BEAST_KEYPAIR_B64=$(cat keypair.json | base64 -w0)
# macOS (BSD base64)
export SOL_BEAST_KEYPAIR_B64=$(cat keypair.json | base64)

# Optional: Custom config path
export SOL_BEAST_CONFIG_PATH="/path/to/config.toml"

# Logging level
export RUST_LOG=info  # or debug, warn, error
```

## Security Best Practices

### Browser Mode
1. ✅ Never share your seed phrase
2. ✅ Only connect to trusted websites
3. ✅ Review transactions before approving
4. ✅ Use separate wallets for trading vs holding
5. ✅ Start with small amounts

### CLI Mode
1. ✅ Never commit keypair.json to git
2. ✅ Use environment variables for sensitive data
3. ✅ Set proper file permissions: `chmod 600 keypair.json`
4. ✅ Use dedicated trading wallet
5. ✅ Test thoroughly in dry-run mode first
6. ✅ Monitor logs regularly
7. ✅ Set reasonable limits (max holdings, buy amount)

## Support & Resources

- **Documentation**: See [ARCHITECTURE.md](./ARCHITECTURE.md)
- **Issues**: GitHub Issues
- **Solana Docs**: https://docs.solana.com
- **Wallet Adapter**: https://github.com/solana-labs/wallet-adapter

## Next Steps

After setup:

1. **Browser Mode Users**:
   - Connect your wallet
   - Configure your strategy in Settings
   - Monitor the New Coins panel
   - Review Holdings and Trades

2. **CLI Mode Users**:
   - Run in dry-run mode for a few hours
   - Review logs and simulated trades
   - Adjust strategy parameters
   - Switch to `--real` mode when confident
   - Monitor via web dashboard

3. **Developers**:
   - Explore the codebase structure
   - Read [ARCHITECTURE.md](./ARCHITECTURE.md)
   - Run tests: `cargo test --workspace`
   - Make improvements and contribute!

Happy trading! 🚀
