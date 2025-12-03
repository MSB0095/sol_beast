# Quick Start Guide

Get Sol Beast up and running in under 5 minutes! This guide covers the fastest path to start using the bot.

## Choose Your Mode

Sol Beast offers two deployment modes:

### 🌐 Option 1: WASM Mode (Recommended for Beginners)

**No backend, no server, runs in your browser!**

1. **Visit the Live Demo**
   ```
   https://msb0095.github.io/sol_beast/
   ```
   Or deploy your own instance to GitHub Pages (see [GitHub Pages Deployment](../deployment/github-pages.md))

2. **Connect Your Wallet**
   - Click the "Connect Wallet" button in the top right
   - Select your preferred Solana wallet (Phantom, Solflare, etc.)
   - Approve the connection

3. **Configure RPC Endpoint**
   - Navigate to **Configuration** tab
   - Enter your RPC URL (must support CORS)
   - Recommended: [Helius](https://helius.dev), [QuickNode](https://quicknode.com), or [Triton](https://triton.one)

4. **Enable Dry Run Mode**
   - Set "Dry Run" to **ON** in Configuration
   - This lets you test without risking real funds

5. **Start the Bot**
   - Click **"Start Bot"** button
   - Watch the logs for detected tokens

### 🖥️ Option 2: Backend Mode (Advanced)

**Full performance with self-hosted backend**

1. **Clone the Repository**
   ```bash
   git clone https://github.com/MSB0095/sol_beast.git
   cd sol_beast
   ```

2. **Configure Settings**
   ```bash
   cp config.example.toml config.toml
   # Edit config.toml with your settings
   ```

3. **Run in Dry Mode (Safe)**
   ```bash
   RUST_LOG=info cargo run
   ```

4. **Access the Frontend**
   ```bash
   cd frontend
   npm install
   npm run dev
   ```
   Open `http://localhost:5173`

## 🎯 First Steps After Setup

### 1. Understand Dry Run Mode

**Always start in dry run mode!** This simulates trading without executing real transactions:

- ✅ Monitors real blockchain events
- ✅ Evaluates tokens using your heuristics
- ✅ Simulates buy/sell decisions
- ❌ Does NOT spend SOL
- ❌ Does NOT execute transactions

### 2. Configure Buy Heuristics

Navigate to **Configuration → Trading Settings** and adjust:

- **Max SOL per Buy** - How much to spend per token (default: 0.1 SOL)
- **Min Market Cap** - Minimum token market cap to consider
- **Max Market Cap** - Maximum token market cap to consider
- **Developer Buy Filter** - Ignore if developer buys too much

### 3. Set Risk Management

Configure your safety limits:

- **Max Portfolio SOL** - Total SOL you're willing to allocate
- **Max Positions** - Maximum number of simultaneous holdings
- **Take Profit %** - When to sell for profit (default: 50%)
- **Stop Loss %** - When to cut losses (default: -30%)

### 4. Watch the Logs

The **Logs** tab shows:
- 🔍 Detected tokens
- ✅ Tokens that pass heuristics
- ❌ Tokens that fail heuristics
- 💰 Simulated trades (in dry run)
- 📊 Position updates

### 5. Monitor Holdings

The **Holdings** tab displays:
- Current positions
- Profit/Loss per position
- Take profit and stop loss levels
- Time held

## 📊 Example Configuration

Here's a conservative starting configuration:

```toml
# Trading Settings
real_mode = false                    # DRY RUN!
max_sol_per_buy = 0.05              # Small test amount
max_portfolio_sol = 0.5             # Limit total exposure
max_positions = 3                    # Few simultaneous positions

# Heuristics
min_market_cap_sol = 1000           # $200k+ at $200/SOL
max_market_cap_sol = 50000          # $10M max
min_holders = 50                     # Require some distribution
max_dev_token_percentage = 20.0     # Dev holds max 20%

# Position Management
take_profit_percentage = 100.0      # Double your money
stop_loss_percentage = -50.0        # Cut losses at -50%
timeout_minutes = 30                # Exit after 30 minutes
```

## ⚡ Pro Tips

### For WASM Mode:
1. **Use a fast RPC** - Helius Pro or QuickNode for best performance
2. **Enable CORS** - Your RPC provider must allow browser requests
3. **Keep the tab active** - Browser may throttle background tabs
4. **Monitor SOL balance** - Check wallet balance regularly

### For Backend Mode:
1. **Use `--release`** - Better performance: `cargo run --release`
2. **Set `RUST_LOG`** - Control log verbosity: `RUST_LOG=info,sol_beast=debug`
3. **Run as service** - Use systemd or Docker for production
4. **Monitor resources** - Watch CPU/memory usage

## 🚨 Before Going Live

**CRITICAL CHECKLIST:**

- [ ] Tested thoroughly in dry run mode
- [ ] Understood all configuration settings
- [ ] Set appropriate risk limits
- [ ] Secured your wallet private keys
- [ ] Started with small amounts
- [ ] Monitored initial trades closely
- [ ] Have a plan to stop the bot if needed

## 🆘 Troubleshooting

### Bot Won't Start
- Check RPC endpoint is accessible
- Verify WebSocket URL is correct
- Ensure wallet is connected (WASM mode)
- Check logs for error messages

### No Tokens Detected
- Verify pump.fun program ID is correct
- Check WebSocket connection status
- Ensure RPC endpoint supports subscriptions
- Look for connection errors in logs

### Can't Execute Trades (WASM)
- Confirm wallet is connected
- Check SOL balance is sufficient
- Verify transaction approval in wallet
- Ensure not in dry run mode (if intending real trades)

## 📚 Next Steps

- **[Installation Guide](./installation.md)** - Detailed installation instructions
- **[Configuration Reference](../configuration/settings-reference.md)** - All settings explained
- **[Architecture Overview](../architecture/overview.md)** - How Sol Beast works
- **[Trading Features](../trading/bot-operation.md)** - Learn about bot operation

## 🎓 Learning Resources

- [Solana Basics](../appendix/solana-basics.md) - Understand Solana blockchain
- [Pump.fun Protocol](../appendix/pumpfun.md) - How pump.fun works
- [Risk Management](../configuration/risk-management.md) - Protect your capital

Ready to dive deeper? Continue to the [Installation Guide](./installation.md) for complete setup instructions.
