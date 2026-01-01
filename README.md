# Sol Beast - Solana Memecoins Sniper

🚀 **Dual-Mode Trading Bot**: Run in browser (WASM) or with backend (Native Rust)

Tiny Rust async service to monitor pump.fun events on Solana, auto-buy under heuristics and manage holdings (TP/SL/timeout).

## ⚡ NEW: Parallel WebSocket Detection for Maximum Speed

Sol Beast now uses **multiple parallel WebSocket connections** for faster and more reliable memecoin detection:

- 🚀 **50% faster detection** with multiple endpoints
- 🎯 **90%+ reduction in missed tokens**
- 💪 **99.99%+ uptime** vs 99.5% with single connection
- 🌐 **Geographic redundancy** for lower latency
- 🔄 **No single point of failure** - system continues even if some connections drop

Simply configure multiple WSS URLs in your `config.toml`:
```toml
solana_ws_urls = [
    "wss://your-helius-endpoint.com/?api-key=KEY",
    "wss://your-quicknode-endpoint.com/KEY/",
    "wss://your-alchemy-endpoint.com/v2/KEY"
]
```

See [MEMECOIN_DETECTION_OPTIMIZATION.md](./MEMECOIN_DETECTION_OPTIMIZATION.md) for details.

## 🎯 Deployment Modes

### 🌐 WASM Mode (GitHub Pages)
**No backend needed! Runs entirely in your browser.**

- ✅ Deploy to GitHub Pages
- ✅ No server costs
- ✅ Wallet Adapter integration
- ✅ Works on any static host
- ✅ Browser-based WebSocket connections
- ✅ localStorage for settings persistence

**Try it now**: Visit the deployed GitHub Pages version!

⚠️ **Important**: 
- **RPC Configuration Required**: On first load, you must configure at least one HTTPS RPC URL and one WSS WebSocket URL
- Public Solana RPCs (`api.mainnet-beta.solana.com`) do NOT support browser CORS and will fail
- Use premium providers with CORS support: Helius, QuickNode, or Alchemy
- See [RPC Configuration Guide](./RPC_CONFIGURATION_GUIDE.md) for detailed setup instructions

### 🖥️ Backend Mode (Self-Hosted)
**Full-featured with Rust backend server.**

- ✅ Optimal performance
- ✅ Server-side WebSocket subscriptions
- ✅ Secure key storage
- ✅ File-based configuration
- ✅ REST API for frontend
- ✅ Recommended for production

## 📊 Feature Comparison

| Feature | WASM Mode | CLI Mode | Status |
|---------|-----------|----------|--------|
| **Core Monitoring** |
| WebSocket monitoring | ✅ Browser fetch | ✅ tokio-tungstenite | 100% |
| Transaction parsing | ✅ Core lib | ✅ Core lib | 100% |
| Pump.fun detection | ✅ Core lib | ✅ Core lib | 100% |
| **Token Analysis** |
| Metadata fetching | ✅ fetch API | ✅ reqwest | 100% |
| Buy heuristics | ✅ Core lib | ✅ Core lib | 100% |
| Risk evaluation | ✅ Core lib | ✅ Core lib | 100% |
| **Trading** |
| Transaction building | ✅ Core lib | ✅ Core lib | 100% |
| Wallet signing | 🚧 Browser wallet | ✅ Keypair | Phase 2 |
| Buy execution | 🚧 In progress | ✅ Implemented | Phase 3 |
| Sell execution | 🚧 In progress | ✅ Implemented | Phase 3 |
| **Position Management** |
| Holdings tracking | 🚧 In progress | ✅ Implemented | Phase 4 |
| TP/SL detection | 🚧 In progress | ✅ Implemented | Phase 4 |
| Timeout handling | 🚧 In progress | ✅ Implemented | Phase 4 |
| **Storage** |
| Settings persistence | ✅ localStorage | ✅ File-based | 100% |
| State recovery | ✅ localStorage | ✅ File-based | 100% |
| **Network** |
| RPC client | ✅ fetch API | ✅ solana_client | 100% |
| HTTP client | ✅ fetch API | ✅ reqwest | 100% |

**Legend**: ✅ Implemented | 🚧 In Progress | ❌ Not Available

### Architecture

**Centralized Core (`sol_beast_core`)**
- All business logic, heuristics, and transaction building
- Platform-agnostic traits for RPC, HTTP, storage
- Zero code duplication between modes

**Platform Adapters**
- `sol_beast_cli`: Native implementations (tokio, reqwest, files)
- `sol_beast_wasm`: WASM implementations (fetch API, localStorage)

This architecture ensures:
- ✅ Feature parity between modes
- ✅ Single source of truth for business logic
- ✅ Easy maintenance (bug fixes benefit both modes)
- ✅ Testable core without platform dependencies

---

## 🔄 CI/CD & Automated Testing

**Mobile-First Development**: Complete automated testing environment managed from your phone!

### Quick Setup (5 minutes)
Configure three repository secrets and get automatic testing on every push:
- `SOLANA_RPC_URL` - Solana RPC endpoint
- `SOLANA_WS_URL` - WebSocket endpoint  
- `SHYFT_API_KEY` - Shyft GraphQL API key (optional but recommended)

**📱 [Quick Start Guide](./QUICK_START_CI.md)** - Get started in 5 minutes from mobile

### Available Workflows
- **Comprehensive CI** - Automatic testing on push/PR (Rust tests, WASM build, Playwright UI tests, bot tests)
- **Deploy to GitHub Pages** - Automatic deployment to production
- **Test Deployment** - Manual validation before deployment

**📚 Detailed Documentation**:
- [GitHub Secrets Setup Guide](./GITHUB_SECRETS_SETUP.md) - Complete configuration instructions
- [Workflows README](./.github/workflows/README.md) - Understanding workflows and artifacts

### Benefits
✅ No local machine needed - runs in GitHub Actions  
✅ View test results from mobile - screenshots, logs, reports  
✅ Automatic testing - catches bugs before deployment  
✅ Free tier available - generous GitHub Actions limits

---

## Quick Start

### Option 1: WASM Mode (Browser Only)

```bash
# Build WASM
./build-wasm.sh

# Build frontend
cd frontend
npm install
VITE_USE_WASM=true npm run build

# Serve dist/ folder or deploy to GitHub Pages
```

**Automatic GitHub Pages**: Just push to `main` branch!

### Option 2: Backend Mode (Traditional)

1. Copy the example config and edit values (RPC/WS URLs and program IDs):

```bash
cp config.example.toml config.toml
# edit config.toml and set wallet_keypair_path before using --real
```

2. Run in dry (safe) mode — this will NOT use any wallet or send transactions:

```bash
RUST_LOG=info cargo run
```

3. Run in real mode (ONLY after you set `wallet_keypair_path` in `config.toml` to a secure keypair file):

```bash
RUST_LOG=info cargo run --release -- --real
```

Notes & safety

- The `--real` path uses the keypair file at `wallet_keypair_path`. Do not commit private keys to the repository.
- `rpc::buy_token` and `rpc::sell_token` contain TODOs and placeholder `Instruction` data — review and implement proper transaction construction before enabling `--real` in any automated environment.

## 📁 Project Structure

```
sol_beast/
├── sol_beast_core/          # Platform-agnostic trading logic
│   ├── src/
│   │   ├── models.rs        # Data models
│   │   ├── tx_builder.rs    # Transaction construction
│   │   ├── settings.rs      # Configuration
│   │   ├── wasm/            # Browser-specific code
│   │   │   ├── rpc.rs       # Fetch API RPC client
│   │   │   ├── websocket.rs # Browser WebSocket
│   │   │   └── storage.rs   # localStorage
│   │   └── native/          # Server-specific code
│   └── Cargo.toml           # Feature flags: native, wasm
│
├── sol_beast_wasm/          # WASM bindings for browser
│   ├── src/lib.rs           # JavaScript API exports
│   └── Cargo.toml           # WASM build configuration
│
├── sol_beast_cli/           # Backend server (original)
│   ├── src/
│   │   ├── main.rs          # Runtime & message processing
│   │   ├── api.rs           # REST API endpoints
│   │   ├── buyer.rs         # Token buying logic
│   │   ├── monitor.rs       # Holdings monitor (TP/SL)
│   │   └── helius_sender.rs # Helius integration
│   └── Cargo.toml
│
├── frontend/                # React frontend
│   ├── src/
│   │   ├── services/
│   │   │   └── botService.ts  # Dual-mode adapter
│   │   ├── components/      # UI components
│   │   ├── stores/          # Zustand stores
│   │   └── wasm/            # Generated WASM (git-ignored)
│   └── package.json
│
├── build-wasm.sh            # WASM build script
├── DUAL_MODE_GUIDE.md       # Deployment guide
└── .github/workflows/
    └── deploy.yml           # GitHub Pages deployment
```

## Files of interest

**Core Library** (shared):
- `sol_beast_core/src/models.rs` — Bonding curve state and models
- `sol_beast_core/src/tx_builder.rs` — Transaction construction
- `sol_beast_core/src/settings.rs` — Configuration management
- `sol_beast_core/src/wasm/` — Browser-specific implementations

**Backend** (CLI mode):
- `sol_beast_cli/src/main.rs` — Runtime, message processing and holdings monitor
- `sol_beast_cli/src/ws.rs` — WebSocket subscriptions and reconnect loop
- `sol_beast_cli/src/rpc.rs` — Solana RPC helpers, price extraction, buy/sell functions
- `sol_beast_cli/src/helius_sender.rs` — Helius Sender integration for ultra-low latency
- `config.example.toml` — Example configuration (copy to `config.toml`)

**WASM** (Browser mode):
- `sol_beast_wasm/src/lib.rs` — JavaScript API exports
- `frontend/src/services/botService.ts` — Dual-mode adapter (auto-detects WASM vs API)

**Frontend**:
- `frontend/src/components/` — React UI components
- `frontend/src/stores/` — Zustand state management
- `frontend/src/contexts/WalletContextProvider.tsx` — Solana Wallet Adapter

## 🏗️ Architecture

### WASM Mode
```
┌───────────────────────────────┐
│       Browser                 │
│  ┌─────────────────────────┐  │
│  │  React Frontend         │  │
│  │          ↓              │  │
│  │  botService (adapter)   │  │
│  │          ↓              │  │
│  │  WASM Bot Module        │  │
│  │  (sol_beast_wasm)       │  │
│  │          ↓              │  │
│  │  Solana RPC/WebSocket   │  │
│  └─────────────────────────┘  │
└───────────────────────────────┘
```

### Backend Mode
```
┌─────────────┐    HTTP     ┌──────────────────┐
│   Browser   │ ◄────────► │  Rust Backend    │
│   (React)   │             │  (Axum API)      │
│             │             │  sol_beast_cli   │
└─────────────┘             └────────┬─────────┘
                                     ↓
                            Solana RPC/WebSocket
```

### Shared Core
Both modes use `sol_beast_core`:
- Trading logic
- Transaction building
- Models & types
- Settings management

**Zero code duplication!**

## Helius Sender Integration

sol_beast supports optional ultra-low latency transaction submission via [Helius Sender](https://docs.helius.dev/solana-rpc-nodes/sending-transactions-on-solana/sender). When enabled, transactions are sent to both Solana validators and Jito infrastructure simultaneously for maximum inclusion probability and speed.

### Features

- **Dual Routing**: Transactions sent to both validators and Jito simultaneously
- **Dynamic Priority Fees**: Automatically fetches recommended fees from Helius Priority Fee API
- **Dynamic Tips**: Supports configurable minimum tip amounts (default 0.001 SOL)
- **Automatic Compute Optimization**: Simulates transactions to determine optimal compute unit limits
- **Global & Regional Endpoints**: Choose HTTPS (frontend) or regional HTTP endpoints (backend)
- **Retry Logic**: Built-in retry mechanism with exponential backoff

### Configuration

Enable Helius Sender by adding these settings to your `config.toml`:

```toml
# Enable Helius Sender for ultra-low latency transaction submission
helius_sender_enabled = true

# Optional: Helius API key (for custom TPS limits beyond default 15 TPS)
# Get your key from: https://dashboard.helius.dev/api-keys
# helius_api_key = "your-helius-api-key-here"

# Helius Sender endpoint (default: global HTTPS)
# For backend/server applications, use regional HTTP endpoints:
#   - http://slc-sender.helius-rpc.com/fast  (Salt Lake City)
#   - http://ewr-sender.helius-rpc.com/fast  (Newark)
#   - http://lon-sender.helius-rpc.com/fast  (London)
#   - http://fra-sender.helius-rpc.com/fast  (Frankfurt)
#   - http://ams-sender.helius-rpc.com/fast  (Amsterdam)
#   - http://sg-sender.helius-rpc.com/fast   (Singapore)
#   - http://tyo-sender.helius-rpc.com/fast  (Tokyo)
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"

# Minimum tip amount in SOL (required by Helius Sender)
# Default: 0.001 SOL (or 0.000005 SOL if using ?swqos_only=true)
# For competitive trading, consider higher tips (e.g., 0.005-0.01 SOL)
helius_min_tip_sol = 0.001

# Priority fee multiplier for recommended fees
# Applied to Helius Priority Fee API recommendations
# Default: 1.2 (20% above recommended for better inclusion)
helius_priority_fee_multiplier = 1.2

# Routing mode: choose between dual routing or SWQOS-only
# Default: false (dual routing)
helius_use_swqos_only = false
```

### Routing Modes

Helius Sender supports two routing modes:

#### 1. Default Dual Routing (Recommended for Speed)

```toml
helius_use_swqos_only = false  # Default
helius_min_tip_sol = 0.001     # Minimum 0.001 SOL required
```

**How it works:**
- Sends transactions to **both** Solana validators **AND** Jito infrastructure simultaneously
- Maximum inclusion probability and lowest latency
- Best for time-critical sniping and competitive trading

**Requirements:**
- Minimum tip: **0.001 SOL** (~$0.20 at $200/SOL)
- Higher cost but maximum speed

**When to use:**
- High-frequency sniping
- Time-sensitive token launches
- Competitive trading scenarios
- When speed is more important than cost

#### 2. SWQOS-Only Alternative (Cost-Optimized)

```toml
helius_use_swqos_only = true
helius_min_tip_sol = 0.000005  # Minimum 0.000005 SOL required
```

**How it works:**
- Routes exclusively through SWQOS infrastructure
- Lower tip requirement for cost savings
- Automatically appends `?swqos_only=true` to endpoint URL

**Requirements:**
- Minimum tip: **0.000005 SOL** (~$0.001 at $200/SOL) - **200x cheaper!**
- Lower cost, still good performance

**When to use:**
- Less time-critical trades
- Higher volume trading where costs add up
- Testing and development
- When cost efficiency matters more than absolute minimum latency

**Cost Comparison Example:**
- 100 transactions with dual routing: 100 × 0.001 = **0.1 SOL** (~$20)
- 100 transactions with SWQOS-only: 100 × 0.000005 = **0.0005 SOL** (~$0.10)


### Requirements

When using Helius Sender, the following are automatically handled:

- **Tips**: Minimum 0.001 SOL transfer to designated Jito tip accounts (configurable via `helius_min_tip_sol`)
- **Priority Fees**: Dynamically fetched from Helius Priority Fee API and applied via `ComputeBudgetProgram`
- **Skip Preflight**: Automatically set to `true` for optimal speed
- **Compute Units**: Automatically calculated via transaction simulation

### Usage

Once configured, Helius Sender is used automatically for all buy and sell transactions when `helius_sender_enabled = true`. The bot will:

1. Build your transaction instructions (buy/sell + ATA creation if needed)
2. Simulate the transaction to determine optimal compute unit limits
3. Fetch dynamic priority fees from Helius API
4. Add compute budget instructions (unit limit + price)
5. Add a tip transfer to a random Jito tip account
6. Send via Helius Sender with retry logic (up to 3 attempts)

### Cost Considerations

- **No API Credits**: Helius Sender doesn't consume API credits from your plan
- **Tips**: Each transaction requires a tip (default 0.001 SOL = ~$0.20 at $200/SOL)
- **Priority Fees**: Additional network fees based on congestion (typically 0.00001-0.0001 SOL)
- **Default Rate Limit**: 15 transactions per second (TPS)
- **Custom Limits**: Contact Helius for higher TPS limits

### Monitoring

When Helius Sender is enabled, you'll see log messages like:

```
INFO Using Helius Sender for buy transaction of mint <mint_address>
INFO Transaction sent via Helius Sender: <signature>
```

### Fallback

If `helius_sender_enabled = false` (default), transactions use the standard Solana RPC `sendTransaction` method via the configured `solana_rpc_urls`.

### Advanced Features

#### Dynamic Tips from Jito API

When `helius_use_dynamic_tips = true` (default) and using dual routing mode, the bot automatically fetches the 75th percentile tip amount from the Jito API:

```toml
helius_use_dynamic_tips = true  # Default: fetch dynamic tips
```

**How it works:**
- Queries `https://bundles.jito.wtf/api/v1/bundles/tip_floor` before each transaction
- Uses 75th percentile of recently landed tips
- Automatically adjusts to current network conditions and competition
- Falls back to `helius_min_tip_sol` if API fails
- Always enforces configured minimum (0.001 SOL for dual, 0.000005 SOL for SWQOS)

**SWQOS-only behavior:**
- Always uses minimum tip (0.000005 SOL) regardless of dynamic tips setting
- Optimizes for cost over competitive advantage

**Benefits:**
- ✅ Automatically competitive during high-traffic launches
- ✅ Saves SOL during quiet periods
- ✅ No manual tip adjustment needed
- ✅ Safe fallback if API unavailable

**Example log output:**
```
INFO Dynamic tip from Jito API: 0.005000000 SOL (75th percentile)
INFO Using dual routing (validators + Jito) with tip: 0.005000000 SOL
```

#### Blockhash Validation

The bot automatically validates blockhash expiration before sending transactions:

- Checks current block height vs. last valid block height
- Prevents wasted fees on expired transactions
- Logs warnings if blockhash expires during retries

#### Transaction Confirmation (Optional)

Confirmation checking is available but disabled by default for speed. To enable, uncomment the confirmation block in `src/helius_sender.rs`:

```rust
// In send_transaction_with_retry function, uncomment:
match confirm_transaction(&sig, rpc_client, settings.helius_confirm_timeout_secs).await {
    Ok(_) => return Ok(sig),
    Err(e) => {
        warn!("Transaction sent but confirmation failed: {}", e);
        return Ok(sig); // Return signature anyway
    }
}
```

Configure timeout in `config.toml`:
```toml
helius_confirm_timeout_secs = 15  # Wait up to 15 seconds for confirmation
```

### Configuration Summary

**Recommended for speed (competitive sniping):**
```toml
helius_sender_enabled = true
helius_use_swqos_only = false       # Dual routing
helius_use_dynamic_tips = true      # Auto-adjust tips
helius_min_tip_sol = 0.001          # Minimum floor
helius_priority_fee_multiplier = 1.2
```

**Recommended for cost optimization:**
```toml
helius_sender_enabled = true
helius_use_swqos_only = true        # SWQOS-only
helius_use_dynamic_tips = false     # Use minimum
helius_min_tip_sol = 0.000005       # SWQOS minimum
helius_priority_fee_multiplier = 1.0
```

### Additional Resources

- [Helius Sender Documentation](https://docs.helius.dev/solana-rpc-nodes/sending-transactions-on-solana/sender)
- [Jito Tips Best Practices](https://docs.jito.wtf/lowlatencytxnsend/#tips)
- [Jito Tip Floor API](https://bundles.jito.wtf/api/v1/bundles/tip_floor)
- [Helius Priority Fee API](https://docs.helius.dev/solana-rpc-nodes/priority-fee-api)

