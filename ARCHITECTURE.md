# Sol Beast Architecture

This file has been moved to `sol_beast_docs/ARCHITECTURE.md`. The original content is preserved there for documentation. 

## Overview

Sol Beast has been transformed into a **dual-mode** trading bot that can run both:
1. **In the browser** using WebAssembly (WASM) with Solana wallet integration
2. **As a CLI/server** using native Rust for advanced users

## Project Structure

```
sol_beast/
├── sol_beast_core/          # Shared core library
│   ├── src/
│   │   ├── error.rs         # Common error types
│   │   ├── models.rs        # Data models (holdings, trades, etc.)
│   │   ├── wallet.rs        # Wallet management & storage
│   │   ├── transaction.rs   # Transaction building
│   │   ├── rpc_client.rs    # RPC client abstraction
│   │   ├── strategy.rs      # Trading strategy (TP/SL/timeout)
│   │   └── lib.rs          # Core library entry
│   └── Cargo.toml
│
├── sol_beast_wasm/          # WASM module for browsers
│   ├── src/
│   │   └── lib.rs          # WASM bindings
│   └── Cargo.toml
│
├── sol_beast_cli/           # CLI application
│   ├── src/
│   │   └── main.rs         # CLI entry point
│   └── Cargo.toml
│
├── frontend/                # React + TypeScript UI
│   ├── src/
│   │   ├── contexts/       # Wallet provider
│   │   ├── store/          # Zustand stores (including WASM)
│   │   ├── components/     # UI components
│   │   └── wasm/           # Generated WASM files (build artifact)
│   └── package.json
│
└── Cargo.toml              # Workspace root
```

## Architecture Modes

### 1. Browser Mode (WASM)

**How it works:**
- User connects their Solana wallet (Phantom, Solflare, etc.)
- WASM module runs trading logic directly in the browser
- User account data is stored locally in browser's localStorage
- All transactions are signed by the user's wallet
- No server required - fully decentralized

**User Flow:**
1. Visit the web app
2. Connect Solana wallet
3. Configure trading settings (TP/SL, buy amount, etc.)
4. Bot monitors pump.fun tokens in real-time
5. User approves transactions via wallet popup
6. Holdings and trade history stored per wallet address

**Benefits:**
- ✅ No server costs
- ✅ Complete user control over funds
- ✅ Private - no data sent to servers
- ✅ Cross-platform (works on any browser)
- ✅ Per-wallet account persistence

### 2. CLI/Server Mode (Native)

**How it works:**
- Traditional server-side bot
- Uses keypair file or environment variables
- Runs continuously monitoring Solana
- REST API + WebSocket for frontend monitoring
- Full automation without user interaction

**User Flow:**
1. Configure `config.toml` with RPC endpoints and wallet
2. Run: `cargo run --release -- --real`
3. Bot runs autonomously
4. Optional: Access web dashboard for monitoring

**Benefits:**
- ✅ Fully automated trading
- ✅ Lower latency (direct server <-> Solana)
- ✅ Advanced features (Helius Sender, etc.)
- ✅ Can run 24/7 on a VPS
- ✅ No wallet popups

## Core Library Features

The `sol_beast_core` library provides platform-agnostic functionality:

### Wallet Management
- Connect/disconnect wallet
- Per-wallet user accounts
- Persistent storage (localStorage in browser, filesystem in CLI)
- Balance tracking

### Trading Strategy
- Configurable TP (take profit) %
- Configurable SL (stop loss) %
- Timeout-based auto-sell
- Price filters and safer sniping mode

### Transaction Building
- Bonding curve PDA calculation
- Token output calculation (constant product formula)
- SOL output calculation for sells
- Slippage tolerance

### RPC Client
- Abstraction over native and WASM RPC calls
- Account data fetching
- Balance queries
- Transaction submission

## WASM Build Process

```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Build WASM module
cd sol_beast_wasm
./wasm-pack-build.sh

# This generates files in frontend/src/wasm/
```

The generated files include:
- `sol_beast_wasm.js` - JavaScript bindings
- `sol_beast_wasm_bg.wasm` - WebAssembly binary
- `sol_beast_wasm.d.ts` - TypeScript definitions

## Frontend Integration

### Wallet Adapter
```typescript
import { WalletProvider } from '@solana/wallet-adapter-react';
import { PhantomWalletAdapter } from '@solana/wallet-adapter-wallets';

// Supports: Phantom, Solflare, Torus, Ledger
```

### WASM Store
```typescript
const { initializeWasm, connectWallet, updateSettings } = useWasmStore();

// Initialize WASM
await initializeWasm();

// Connect wallet
const account = await connectWallet(walletAddress);

// Update trading settings
await updateSettings({
  tp_percent: 30,
  sl_percent: -20,
  buy_amount: 0.1,
});
```

## Data Models

### UserAccount
```rust
pub struct UserAccount {
    pub wallet_address: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub total_trades: u64,
    pub total_profit_loss: f64,
    pub settings: UserSettings,
}
```

### Holding
```rust
pub struct Holding {
    pub mint: String,
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: DateTime<Utc>,
    pub metadata: Option<OffchainMetadata>,
    pub onchain: Option<OnchainFullMetadata>,
}
```

### TradeRecord
```rust
pub struct TradeRecord {
    pub mint: String,
    pub trade_type: String, // "buy" or "sell"
    pub timestamp: DateTime<Utc>,
    pub amount_sol: f64,
    pub price_per_token: f64,
    pub profit_loss: Option<f64>,
}
```

## Configuration

### Browser Mode
Settings are stored per wallet in localStorage and managed through the UI.

### CLI Mode
Configure via `config.toml`:
```toml
[trading]
tp_percent = 30.0
sl_percent = -20.0
timeout_secs = 3600
buy_amount = 0.1

[wallet]
wallet_keypair_path = "./keypair.json"
# Or use environment variable: SOL_BEAST_KEYPAIR_B64

[network]
solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]
solana_ws_urls = ["wss://api.mainnet-beta.solana.com"]
```

## Security Considerations

### Browser Mode
- Private keys never leave the wallet extension
- User must approve each transaction
- Account data stored only in browser localStorage
- No server-side tracking

### CLI Mode
- Protect your keypair file
- Use environment variables for sensitive data
- Consider using separate wallets for trading
- Monitor logs for suspicious activity

## Development

### Build CLI
```bash
cargo build --release -p sol_beast_cli
```

### Build WASM
```bash
cd sol_beast_wasm
wasm-pack build --target web
```

### Run Tests
```bash
# Core library
cargo test -p sol_beast_core

# All packages
cargo test --workspace
```

### Frontend Development
```bash
cd frontend
npm install
npm run dev
```

## Deployment

### Browser Mode
1. Build WASM module
2. Build frontend: `npm run build`
3. Deploy `frontend/dist` to any static host (Vercel, Netlify, etc.)

### CLI Mode
1. Build: `cargo build --release -p sol_beast_cli`
2. Copy binary to server
3. Configure `config.toml`
4. Run: `./sol_beast --real`

## Future Enhancements

- [ ] Service Worker for background trading in browser
- [ ] IndexedDB for larger datasets
- [ ] P2P signaling for shared strategies
- [ ] Multi-wallet portfolio management
- [ ] Advanced charting and analytics
- [ ] Mobile app (React Native with same WASM core)
- [ ] Hardware wallet support
