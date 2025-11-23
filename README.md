# sol_beast

**A dual-mode Solana trading bot** for monitoring pump.fun token launches with automated buy/sell strategies.

## 🚀 Two Ways to Run

### 🌐 Browser Mode (New!)
Trade directly from your browser using WebAssembly (WASM) and your Solana wallet:
- ✅ No server needed - fully decentralized
- ✅ Your keys, your control (wallet extension)
- ✅ Cross-platform (any device with a browser)
- ✅ Settings saved per wallet address
- ✅ Local trade history and holdings

### 🖥️ CLI/Server Mode
Traditional automated trading bot for power users:
- ✅ Fully automated 24/7 trading
- ✅ Lower latency (direct server ↔ Solana)
- ✅ Advanced features (Helius Sender, etc.)
- ✅ REST API + Web dashboard
- ✅ Enterprise-grade reliability

## Quick Start

### Browser Mode (5 minutes)

```bash
# 1. Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# 2. Build WASM module
cd sol_beast_wasm && ./wasm-pack-build.sh && cd ..

# 3. Start frontend
cd frontend && npm install && npm run dev

# 4. Open http://localhost:5173 and connect your wallet!
```

### CLI Mode (Traditional)

```bash
# 1. Copy and configure settings
cp config.example.toml config.toml
# Edit config.toml and set your wallet

# 2. Test in dry-run mode (no real transactions)
RUST_LOG=info cargo run

# 3. Run for real (careful!)
RUST_LOG=info cargo run --release -- --real
```

## 📚 Documentation

- **[SETUP.md](./SETUP.md)** - Detailed setup instructions for both modes
- **[ARCHITECTURE.md](./ARCHITECTURE.md)** - Technical architecture and design
- **[config.example.toml](./config.example.toml)** - Configuration reference

## 🏗️ Project Structure

```
sol_beast/
├── sol_beast_core/      # Shared Rust library (native + WASM)
├── sol_beast_wasm/      # WASM bindings for browser
├── sol_beast_cli/       # CLI application (server mode)
├── frontend/            # React + TypeScript dashboard
└── src/                 # Legacy source (being migrated)
```

## ✨ Features

### Trading Strategy
- ✅ Configurable take-profit (TP) percentage
- ✅ Configurable stop-loss (SL) percentage
- ✅ Timeout-based auto-sell
- ✅ Safer sniping filters (price, liquidity, etc.)
- ✅ Slippage tolerance

### Token Monitoring
- ✅ Real-time pump.fun token launches
- ✅ Automatic metadata fetching
- ✅ Bonding curve analysis
- ✅ Multi-websocket support for reliability

### Browser Mode Extras
- ✅ Wallet adapter (Phantom, Solflare, etc.)
- ✅ Per-wallet account persistence
- ✅ Local trade history
- ✅ Real-time holdings tracking
- ✅ User-friendly configuration UI

### CLI Mode Extras
- ✅ Helius Sender integration
- ✅ Dynamic priority fees
- ✅ REST API for monitoring
- ✅ Web dashboard
- ✅ Comprehensive logging

## 🔐 Security

### Browser Mode
- Private keys stay in your wallet extension
- No data sent to external servers
- You approve every transaction
- Settings stored locally in your browser

### CLI Mode
- Never commit private keys
- Use environment variables for secrets
- Test in dry-run mode first
- Use dedicated trading wallets

## 🛠️ Development

```bash
# Test core library
cargo test -p sol_beast_core

# Test all packages
cargo test --workspace

# Build for production
cargo build --release

# Format code
cargo fmt --all

# Lint
cargo clippy --all-targets
```

## Notes & Safety

- **Browser Mode**: You control your wallet and approve all transactions
- **CLI Mode**: The `--real` flag uses your configured keypair - test thoroughly first!
- Start with small amounts to test the strategy
- Monitor your positions regularly
- Set reasonable limits (max holdings, buy amount)

## Files of Interest

- `sol_beast_core/` - Platform-agnostic trading logic
- `sol_beast_wasm/` - WASM bindings for browsers
- `frontend/src/components/` - React UI components
- `frontend/src/store/wasmStore.ts` - WASM integration
- `src/main.rs` - Legacy CLI runtime (being refactored)
- `config.example.toml` - Configuration template

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

