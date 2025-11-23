# Quick Start Guide

Choose your mode and get started in minutes!

## 🌐 Browser Mode (Easiest)

**Perfect for:** Manual trading, trying it out, personal wallets

```bash
# Install wasm-pack (one-time setup)
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Build & run
cd sol_beast_wasm && ./wasm-pack-build.sh
cd ../frontend && npm install && npm run dev

# Open http://localhost:5173 and connect your wallet!
```

**What you get:**
- ✅ Trade directly from browser
- ✅ Use your Phantom/Solflare wallet
- ✅ No server needed
- ✅ Settings saved per wallet

---

## 🖥️ CLI Mode (Most Powerful)

**Perfect for:** Automated trading, 24/7 operation, advanced users

```bash
# Setup config
cp config.example.toml config.toml
nano config.toml  # Add your wallet and settings

# Test first (no real trades)
RUST_LOG=info cargo run -p sol_beast_cli

# Go live (careful!)
RUST_LOG=info cargo run -p sol_beast_cli --release -- --real
```

**What you get:**
- ✅ Fully automated trading
- ✅ Advanced features (Helius, priority fees)
- ✅ REST API + dashboard
- ✅ 24/7 operation

---

## ⚙️ Key Settings

### Browser Mode (UI)
1. Connect wallet
2. Go to Settings tab
3. Configure:
   - **Take Profit:** 30% (default)
   - **Stop Loss:** -20% (default)
   - **Buy Amount:** 0.1 SOL
   - **Max Holdings:** 10 coins

### CLI Mode (`config.toml`)
```toml
# Trading Strategy
tp_percent = 30.0       # Take profit at +30%
sl_percent = -20.0      # Stop loss at -20%
buy_amount = 0.1        # Buy 0.1 SOL worth
timeout_secs = 3600     # Auto-sell after 1 hour

# Wallet (choose one)
wallet_keypair_path = "./keypair.json"
# OR
# wallet_private_key_string = "your_base58_key"
# OR use env: SOL_BEAST_KEYPAIR_B64

# Safety
enable_safer_sniping = true
max_held_coins = 100
```

---

## 🔐 Security Checklist

### Browser Mode
- [ ] Use wallet extension (Phantom, Solflare)
- [ ] Review transactions before approving
- [ ] Start with small amounts
- [ ] Use separate wallet for trading

### CLI Mode
- [ ] Never commit `keypair.json` to git
- [ ] Test in dry-run mode first
- [ ] Use environment variables for keys
- [ ] Monitor logs regularly
- [ ] Set reasonable limits

---

## 📊 Monitoring

### Browser Mode
- Check "Holdings" tab for open positions
- View "Trades" for history
- Watch "New Coins" for opportunities

### CLI Mode
```bash
# View logs
RUST_LOG=info cargo run -p sol_beast_cli

# Use web dashboard
cd frontend && npm run dev
# Visit http://localhost:5173

# Check API
curl http://localhost:8080/stats
```

---

## 🚨 Troubleshooting

### Browser Mode

**Wallet won't connect?**
- Install wallet extension
- Refresh page
- Try different wallet

**WASM not loading?**
```bash
cd sol_beast_wasm
./wasm-pack-build.sh
```

### CLI Mode

**"No wallet configured"?**
- Set `wallet_keypair_path` in config.toml
- Or use `SOL_BEAST_KEYPAIR_B64` env var

**Connection timeouts?**
- Try different RPC endpoint
- Check internet connection

---

## 📚 Learn More

- **[SETUP.md](./SETUP.md)** - Detailed setup instructions
- **[ARCHITECTURE.md](./ARCHITECTURE.md)** - Technical deep dive
- **[MIGRATION.md](./MIGRATION.md)** - Upgrading from old version
- **[README.md](./README.md)** - Project overview

---

## 🎯 Quick Commands

```bash
# Build everything
cargo build --release --workspace

# Test everything
cargo test --workspace

# Build WASM
cd sol_beast_wasm && ./wasm-pack-build.sh

# Run CLI (dry-run)
cargo run -p sol_beast_cli

# Run CLI (real mode)
cargo run -p sol_beast_cli --release -- --real

# Start frontend
cd frontend && npm run dev

# Check core library
cargo check -p sol_beast_core

# Run core tests
cargo test -p sol_beast_core
```

---

## 💡 Tips & Tricks

### Strategy Tips
- Start with lower TP/SL (10% / -5%)
- Increase as you gain confidence
- Use safer sniping filters
- Set max holdings limit
- Monitor first trades closely

### Performance Tips
- Use regional RPC endpoints
- Enable Helius Sender (CLI mode)
- Optimize compute units
- Monitor network congestion

### Safety Tips
- **Always test in dry-run mode first**
- Start with small amounts (0.01 SOL)
- Use dedicated trading wallet
- Monitor your positions regularly
- Set reasonable timeouts

---

## 🔥 Example Workflows

### Conservative Trader (Browser)
```
1. Connect wallet with small amount (0.5 SOL)
2. Set TP: 15%, SL: -10%
3. Buy amount: 0.01 SOL
4. Max holdings: 3
5. Monitor actively, approve each trade
```

### Aggressive Automation (CLI)
```
1. Dedicated wallet with 10 SOL
2. Set TP: 50%, SL: -30%
3. Buy amount: 0.1 SOL
4. Enable Helius Sender
5. Let it run 24/7
```

### Balanced Hybrid (Both)
```
CLI:
- Conservative automated strategy
- Small positions (0.05 SOL)
- Longer timeouts (2 hours)

Browser:
- Aggressive manual snipes
- Personal wallet
- Larger positions (0.2 SOL)
```

---

## ⚡ One-Liners

**Start browser mode now:**
```bash
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh && cd sol_beast_wasm && ./wasm-pack-build.sh && cd ../frontend && npm install && npm run dev
```

**Test CLI immediately:**
```bash
cp config.example.toml config.toml && RUST_LOG=info cargo run -p sol_beast_cli
```

**Run everything:**
```bash
cargo test --workspace && cargo build --release --workspace
```

---

## 🎉 You're Ready!

Pick your mode, follow the steps, and start trading. Good luck! 🚀

**Remember:** 
- Test first with dry-run mode
- Start with small amounts
- Monitor your positions
- Adjust strategy as needed

**Questions?** Check the docs or open an issue on GitHub!
