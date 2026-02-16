# Getting Started

This guide will help you get SOL BEAST up and running in less than 10 minutes.

## Prerequisites

Before you begin, ensure you have the following installed:

### Required Software

1. **Rust 1.70+**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
   Verify installation:
   ```bash
   rustc --version
   ```

2. **Node.js 18+**
   - Download from [nodejs.org](https://nodejs.org/)
   - Verify installation:
   ```bash
   node --version
   npm --version
   ```

3. **Solana CLI** (Optional but recommended)
   ```bash
   sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
   ```

### Required Resources

- **Solana Wallet** with SOL for trading
- **RPC Endpoint** (free tier from Helius, QuickNode, or public RPC)
- **Optional**: Helius API key for advanced features

## Installation

### Step 1: Clone the Repository

```bash
git clone https://github.com/MSB0095/sol_beast.git
cd sol_beast
```

### Step 2: Configuration

Copy the example configuration:
```bash
cp config.example.toml config.toml
```

Edit `config.toml` with your settings:

```toml
# Wallet Configuration (IMPORTANT: Use a dedicated trading wallet)
wallet_keypair_path = "/path/to/your/keypair.json"

# Solana RPC URLs (add your endpoints)
solana_rpc_urls = [
    "https://api.mainnet-beta.solana.com",  # Public (slow)
    # "https://rpc.helius.xyz/?api-key=YOUR_KEY",  # Recommended
]

# Trading Parameters
buy_amount_sol = 0.05              # Amount per trade
max_slippage_bps = 500             # 5% max slippage
take_profit_percentage = 50.0      # Exit at +50%
stop_loss_percentage = 20.0        # Exit at -20%
timeout_seconds = 300              # Exit after 5 min

# Heuristic Filters
min_token_age_seconds = 60         # Skip tokens < 60s old
min_holder_count = 50              # Require 50+ holders
min_liquidity_sol = 10.0           # Require 10+ SOL liquidity
```

::: warning Wallet Security
**NEVER** commit your keypair file to version control. Use a dedicated trading wallet with only the funds you're willing to risk.
:::

### Step 3: Install Frontend Dependencies

```bash
cd frontend
npm install
cd ..
```

## First Run (Dry Mode)

Always test in dry mode first to ensure everything works without risking capital:

```bash
RUST_LOG=info cargo run
```

You should see output like:
```
INFO Starting sol_beast in DRY MODE
INFO WebSocket connected to pump.fun
INFO Monitoring events...
```

::: tip Dry Mode Benefits
Dry mode:
- ✅ Monitors events and evaluates heuristics
- ✅ Logs what trades WOULD be made
- ❌ Does NOT send transactions
- ❌ Does NOT require wallet setup
:::

### Launch the Dashboard

In a separate terminal:
```bash
cd frontend
npm run dev
```

Visit `http://localhost:3000` to see the dashboard.

## Verify Setup

### Backend Health Check

```bash
curl http://localhost:8080/api/health
```

Expected response:
```json
{
  "status": "healthy",
  "mode": "dry",
  "uptime": 123
}
```

### Frontend Connection

1. Open `http://localhost:3000`
2. Check the status indicator (should be green/online)
3. Verify the theme switcher works
4. Check that bot status shows "DRY MODE"

## Going Live (Real Mode)

::: danger Warning
Only proceed when you're confident in your configuration and have tested thoroughly in dry mode.
:::

### Prerequisites for Real Mode

- [ ] Wallet keypair file exists and is secure
- [ ] Wallet has sufficient SOL (trade amount + fees + tips)
- [ ] Configuration values are tested and sensible
- [ ] RPC endpoints are reliable and fast
- [ ] You understand the risks

### Enable Real Trading

1. **Secure your wallet**:
   ```bash
   chmod 600 /path/to/your/keypair.json
   ```

2. **Verify wallet balance**:
   ```bash
   solana balance /path/to/your/keypair.json
   ```

3. **Run in real mode**:
   ```bash
   RUST_LOG=info cargo run --release -- --real
   ```

4. **Monitor actively**: Keep the dashboard open and watch all trades

### First Real Trade Checklist

Start with conservative settings:
- [ ] Small trade amount (0.01-0.05 SOL)
- [ ] Wide stop-loss (30-50%)
- [ ] Reasonable take-profit (30-50%)
- [ ] Short timeout (60-120 seconds)
- [ ] Monitor every trade manually

## Troubleshooting

### Backend Won't Start

**Error**: `Config file not found`
- **Solution**: Ensure `config.toml` exists in the project root

**Error**: `Wallet keypair not found`
- **Solution**: Set correct path in `wallet_keypair_path`

**Error**: `Failed to connect to RPC`
- **Solution**: Check your RPC URLs and network connection

### Frontend Won't Connect

**Issue**: Dashboard shows "Offline"
- Check backend is running on port 8080
- Verify no firewall is blocking the connection
- Check browser console for errors

### Transactions Failing

**Issue**: "Insufficient SOL"
- Ensure wallet has enough SOL for trade + fees + tips
- With Helius Sender enabled, add 0.001 SOL per trade for tips

**Issue**: "Blockhash expired"
- Use faster RPC endpoint
- Enable Helius Sender for better timing

## Next Steps

Now that you're up and running:

1. **[Configure Trading Parameters](/guide/trading-parameters)** - Optimize for your strategy
2. **[Enable Helius Sender](/guide/helius-sender)** - Get ultra-low latency
3. **[Understand the Dashboard](/guide/dashboard)** - Master the interface
4. **[Learn Risk Management](/advanced/risk-management)** - Protect your capital

---

::: info Need Help?
- Check the [FAQ](/guide/faq)
- Visit [Troubleshooting](/guide/troubleshooting)
- Join our [Discord](https://discord.gg/ZwyMw3HaDp)
- Open an [Issue](https://github.com/MSB0095/sol_beast/issues)
:::
