# Quick Start Guide - Browser Version

Get sol_beast running in your browser in 5 minutes!

## Prerequisites

- Web browser with wallet extension installed (Phantom, Solflare, etc.)
- No server or Rust installation required!

## Option 1: Use GitHub Pages (Simplest)

1. **Fork this repository** on GitHub

2. **Enable GitHub Pages**
   - Go to Settings → Pages
   - Source: GitHub Actions
   - Click Save

3. **Wait for deployment** (2-3 minutes)
   - Check the Actions tab for deployment status

4. **Visit your site**
   - URL: `https://[your-username].github.io/sol_beast/`

5. **Connect your wallet**
   - Click "Select Wallet" button
   - Choose your wallet (Phantom recommended)
   - Approve the connection

6. **Start trading!**
   - Configure your settings in the Configuration panel
   - Your settings are automatically saved per wallet

## Option 2: Run Locally

### Step 1: Clone the repository

```bash
git clone https://github.com/MSB0095/sol_beast.git
cd sol_beast/frontend
```

### Step 2: Install dependencies

```bash
npm install
```

### Step 3: Configure environment (optional)

```bash
cp .env.example .env
# Edit .env to customize RPC endpoint, etc.
```

### Step 4: Start development server

```bash
npm run dev
```

### Step 5: Open in browser

Visit: http://localhost:3000

### Step 6: Connect wallet

1. Click "Select Wallet" in the header
2. Choose your wallet provider
3. Approve the connection
4. Your settings will automatically load (or defaults if first time)

## Configuration

### Using Public RPC (Default)

The app uses Solana's public RPC by default. This works but has rate limits.

**Pros:**
- Free
- No configuration needed

**Cons:**
- Rate limited
- May be slow during peak times

### Using Private RPC (Recommended for Active Trading)

For better performance, use a paid RPC provider:

1. Get an API key from:
   - [Helius](https://helius.dev) - Recommended
   - [QuickNode](https://quicknode.com)
   - [Alchemy](https://alchemy.com)

2. Update your `.env` file:
```env
VITE_SOLANA_RPC_URL=https://mainnet.helius-rpc.com/?api-key=YOUR_KEY
VITE_SOLANA_WS_URL=wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY
```

3. Restart the dev server

## Understanding Per-Wallet Accounts

Each wallet you connect gets its own account:

```
Wallet A (GH3k...): 
  - Buy Amount: 0.5 SOL
  - TP: 50%
  - SL: -15%
  
Wallet B (5jX2...):
  - Buy Amount: 0.1 SOL  
  - TP: 30%
  - SL: -20%
```

Settings are stored in your browser's localStorage and automatically restored when you reconnect the same wallet.

## Your Settings

When you connect a wallet for the first time, default settings are created:

- **Buy Amount**: 0.1 SOL
- **Take Profit**: 30%
- **Stop Loss**: -20%
- **Max Holdings**: 10
- **Safer Sniping**: Enabled

You can change these in the Configuration panel. Changes are auto-saved.

## Safety Features

### Dry Run Mode (Default)

By default, the app runs in **simulation mode**:
- No real transactions are sent
- You can test the interface safely
- No SOL will be spent

### Safer Sniping (Recommended)

When enabled, the app checks:
- ✓ Minimum tokens received (prevents buying expensive/late tokens)
- ✓ Maximum price per token
- ✓ Liquidity filters (bonding curve SOL reserves)

## Switching Networks

To use **devnet** for testing:

1. Edit `.env`:
```env
VITE_SOLANA_NETWORK=devnet
VITE_SOLANA_RPC_URL=https://api.devnet.solana.com
```

2. Switch your wallet to devnet
3. Get devnet SOL from a faucet
4. Restart the app

## Troubleshooting

### Wallet not connecting?
- Make sure your wallet extension is installed and unlocked
- Try refreshing the page
- Check browser console for errors (F12)

### Settings not saving?
- Ensure cookies/localStorage are enabled
- Try a different browser
- Clear cache and reconnect wallet

### "Connection failed" errors?
- Check your internet connection
- Verify RPC endpoint is working
- Try a different RPC provider

### Transactions failing?
- Ensure wallet has sufficient SOL for gas fees
- Check Solana network status: https://status.solana.com
- Verify you're on the correct network (mainnet/devnet)

## Advanced: Customize the Trading Engine

The client-side trading engine is in `frontend/src/services/tradingEngine.ts`.

You can customize:
- Token detection logic
- Buy/sell criteria
- TP/SL thresholds
- Risk management rules

See `WASM_DEPLOYMENT.md` for detailed architecture documentation.

## Support

- 📖 Full documentation: [WASM_DEPLOYMENT.md](./WASM_DEPLOYMENT.md)
- 🐛 Report issues: [GitHub Issues](https://github.com/MSB0095/sol_beast/issues)
- 💬 Ask questions: [GitHub Discussions](https://github.com/MSB0095/sol_beast/discussions)

## Next Steps

1. ✅ Connect your wallet
2. ✅ Configure your trading parameters
3. ✅ Test in dry run mode
4. ✅ Monitor for new tokens
5. ⚠️ Enable real mode (use with caution!)

**Happy trading! 🚀**

---

**⚠️ Risk Warning**: Trading meme coins involves substantial risk. Only trade with funds you can afford to lose. Always test in dry run mode first.
