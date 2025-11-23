# Browser-Based Deployment with Wallet Authentication

This document explains how sol_beast has been transformed to run in the browser with Solana wallet integration for serverless user management.

## Architecture Overview

### Previous Architecture (Server-Based)
- Rust backend running on a server
- REST API for frontend communication
- Server-side wallet management
- Centralized state management

### New Architecture (Browser-Based)
- Frontend runs entirely in the browser
- Solana wallet adapter for authentication
- Client-side trading engine
- Per-wallet user settings stored in localStorage
- Direct connection to Solana RPC from browser
- No backend server required (serverless)

## Features

### 🔐 Wallet-Based Authentication
- **Multi-Wallet Support**: Phantom, Solflare, Backpack, Coinbase Wallet, Ledger
- **Auto-Connect**: Automatically reconnects to previously used wallet
- **Persistent Sessions**: Each wallet gets its own persistent account

### 💾 Per-Wallet User Settings
Each connected wallet has its own:
- Trading parameters (buy amount, TP/SL percentages)
- Safety settings (min tokens, max price, liquidity filters)
- Theme preferences
- Trading history
- Holdings data

Settings are automatically saved and restored when the same wallet connects again.

### 🌐 GitHub Pages Deployment
The application is fully static and can be deployed to GitHub Pages:
- No server required
- Zero hosting costs
- Automatic deployment via GitHub Actions
- Global CDN distribution

## Setup Instructions

### Local Development

1. **Install Dependencies**
```bash
cd frontend
npm install
```

2. **Configure Environment**
Create `.env` file in frontend directory:
```env
VITE_SOLANA_NETWORK=mainnet-beta
VITE_SOLANA_RPC_URL=https://api.mainnet-beta.solana.com
```

3. **Run Development Server**
```bash
npm run dev
```

4. **Connect Wallet**
- Click "Select Wallet" button in the header
- Choose your wallet (Phantom, Solflare, etc.)
- Approve the connection
- Your settings will automatically load

### GitHub Pages Deployment

1. **Enable GitHub Pages**
   - Go to repository Settings → Pages
   - Source: GitHub Actions

2. **Push to Main Branch**
   - The GitHub Action will automatically build and deploy
   - Workflow: `.github/workflows/deploy-gh-pages.yml`

3. **Access Your Deployment**
   - URL: `https://[username].github.io/[repository]/`
   - Example: `https://MSB0095.github.io/sol_beast/`

## How It Works

### User Session Management

```typescript
// When wallet connects
const publicKey = wallet.publicKey.toString()

// Load or create user session
if (!userSessions[publicKey]) {
  // First time connecting with this wallet
  userSessions[publicKey] = {
    buyAmount: 0.1,
    tpPercent: 30.0,
    slPercent: -20.0,
    // ... default settings
    createdAt: new Date().toISOString(),
    lastActive: new Date().toISOString()
  }
} else {
  // Returning user - load their settings
  const settings = userSessions[publicKey]
  // Apply settings to trading engine
}
```

### Trading Engine (Client-Side)

```typescript
// Browser-based trading engine
class TradingEngine {
  // Connect to Solana WebSocket
  startTokenMonitoring() {
    this.websocket = new WebSocket(SOLANA_WS_URL)
    
    // Subscribe to pump.fun program logs
    this.websocket.send({
      method: 'logsSubscribe',
      params: [{
        mentions: ['6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P']
      }]
    })
  }
  
  // Execute trades using connected wallet
  async executeBuy(mint: string) {
    const transaction = await buildBuyTransaction(mint)
    const signature = await wallet.sendTransaction(transaction)
    return signature
  }
}
```

## Security Considerations

### ✅ Safe
- Private keys never leave the wallet
- All transactions signed by user's wallet
- No backend to compromise
- Settings stored locally (can be cleared)

### ⚠️ Important Notes
- **RPC Rate Limits**: Public RPC endpoints have rate limits
  - Consider using paid RPC for production
  - Helius, QuickNode, Alchemy offer higher limits
  
- **Browser Security**: 
  - Only connect to trusted frontends
  - Verify the domain before connecting wallet
  - Check transaction details before signing

- **Data Privacy**:
  - Settings stored in browser localStorage
  - Clear browser data = lose settings
  - Export/backup feature recommended for production

## Configuration

### Environment Variables

**Frontend (`frontend/.env`)**
```env
# Solana Network (mainnet-beta, devnet, testnet)
VITE_SOLANA_NETWORK=mainnet-beta

# Custom RPC URL (optional, uses Solana public RPC if not set)
VITE_SOLANA_RPC_URL=https://api.mainnet-beta.solana.com

# WebSocket URL (optional)
VITE_SOLANA_WS_URL=wss://api.mainnet-beta.solana.com

# Base URL for GitHub Pages (optional)
VITE_BASE_URL=/sol_beast/
```

### Recommended RPC Providers

For production use, consider:
- **Helius**: https://helius.dev (Recommended for high-volume)
- **QuickNode**: https://quicknode.com
- **Alchemy**: https://alchemy.com
- **Triton**: https://triton.one

## User Flow

1. **Visit Application**
   - User lands on the frontend (GitHub Pages)
   - No wallet connected yet

2. **Connect Wallet**
   - User clicks "Select Wallet"
   - Chooses wallet provider (Phantom, etc.)
   - Approves connection

3. **Load Settings**
   - Application checks localStorage for wallet's settings
   - If first time: initialize with defaults
   - If returning: restore previous settings

4. **Configure Trading**
   - User adjusts settings via UI
   - Changes auto-save to localStorage (keyed by wallet address)

5. **Start Trading**
   - Client-side engine connects to Solana WebSocket
   - Monitors for new tokens
   - Executes trades using connected wallet

6. **Disconnect**
   - Settings remain in localStorage
   - Will restore on next connection

## Development Roadmap

### Phase 1: Core Wallet Integration ✅
- [x] Wallet adapter integration
- [x] Per-wallet session management
- [x] Settings persistence

### Phase 2: Client-Side Trading Engine 🚧
- [x] WebSocket connection to Solana
- [ ] Token detection logic
- [ ] Buy transaction building
- [ ] Sell transaction building
- [ ] TP/SL monitoring

### Phase 3: Enhanced Features 📋
- [ ] Settings export/import
- [ ] Transaction history export
- [ ] Mobile-responsive wallet UI
- [ ] Multiple wallet management
- [ ] Settings sync across devices (optional backend)

### Phase 4: Production Readiness 📋
- [ ] Rate limiting handling
- [ ] Error recovery
- [ ] Transaction retry logic
- [ ] Comprehensive testing
- [ ] Security audit

## Troubleshooting

### Wallet Not Connecting
- Ensure wallet extension is installed
- Check if wallet is unlocked
- Try refreshing the page
- Check browser console for errors

### Settings Not Saving
- Check browser localStorage is enabled
- Ensure cookies/site data not blocked
- Try clearing cache and reconnecting

### Transactions Failing
- Check wallet has sufficient SOL for gas
- Verify RPC endpoint is responsive
- Check Solana network status
- Review transaction details in wallet

## Support

For issues or questions:
- GitHub Issues: https://github.com/MSB0095/sol_beast/issues
- Check wallet provider documentation
- Verify Solana RPC status

## License

Same as main project license.
