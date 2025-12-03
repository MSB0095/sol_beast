# WASM Mode Features & User Guide

Complete guide to using Sol Beast in WASM mode (browser-based, no backend required).

## 🎯 Overview

Sol Beast WASM mode runs entirely in your browser using WebAssembly, enabling:
- ✅ **No backend server required** - Deploy to GitHub Pages or any static host
- ✅ **Browser wallet integration** - Use Phantom, Solflare, Torus, or Ledger
- ✅ **Real-time monitoring** - Detect new pump.fun tokens as they launch
- ✅ **Automated evaluation** - Apply buy heuristics (liquidity, price, supply checks)
- ✅ **One-click trading** - Buy tokens with wallet approval
- ✅ **Position management** - Track holdings with TP/SL/timeout monitoring
- ✅ **Automatic selling** - Sell when profit target, stop loss, or timeout triggers

## 📊 Feature Status

| Feature | Status | Description |
|---------|--------|-------------|
| **Core Monitoring** | ✅ 100% | WebSocket monitoring of pump.fun transactions |
| **Token Detection** | ✅ 100% | Parse transactions, extract mint addresses |
| **Metadata Fetching** | ✅ 100% | Get token name, symbol, image, description |
| **Buy Heuristics** | ✅ 100% | Evaluate tokens against configurable criteria |
| **Price Fetching** | ✅ 100% | Real-time bonding curve price calculation |
| **Liquidity Tracking** | ✅ 100% | SOL liquidity from bonding curve |
| **Wallet Integration** | ✅ 100% | Phantom, Solflare, Torus, Ledger support |
| **Buy Transactions** | ✅ 100% | Build, sign, submit buy transactions |
| **Holdings Management** | ✅ 100% | Track positions in localStorage |
| **TP/SL/Timeout** | ✅ 100% | Monitor positions, trigger sell alerts |
| **Sell Transactions** | ✅ 100% | Build, sign, submit sell transactions |
| **Toast Notifications** | ✅ 100% | Modern, non-blocking UI feedback |
| **Settings Persistence** | ✅ 100% | Save/load settings from localStorage |
| **Trade History** | ⏳ Planned | Display completed trades with P&L |
| **Export to CSV** | ⏳ Planned | Export trade history |

**Overall Completion**: ~92% (Core trading functionality 100% complete)

## 🚀 Getting Started

### Prerequisites

1. **RPC Endpoint with Browser Support**
   - ⚠️ **Critical**: Public Solana RPC doesn't support browser WebSockets
   - Required: RPC provider with CORS support
   - Recommended providers:
     - **Helius**: `wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY`
     - **QuickNode**: `wss://your-endpoint.quiknode.pro/YOUR_KEY/`
     - **Alchemy**: Similar format with API key

2. **Browser Wallet**
   - Supported: Phantom, Solflare, Torus, Ledger
   - Install browser extension before using

### Step 1: Configure Settings

1. Click **Configuration** tab
2. Set **Solana WebSocket URLs**: `wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY`
3. Set **Solana RPC URLs**: `https://mainnet.helius-rpc.com/?api-key=YOUR_KEY`
4. Configure buy settings:
   - **Buy Amount**: SOL to spend per token (e.g., 0.01)
   - **Slippage BPS**: Acceptable slippage in basis points (e.g., 500 = 5%)
   - **Take Profit %**: Profit target to trigger sell (e.g., 100 = 2x)
   - **Stop Loss %**: Maximum loss before selling (e.g., -50 = -50%)
   - **Timeout Seconds**: Max hold time before selling (e.g., 300 = 5 min)
5. Configure heuristics:
   - **Min Liquidity SOL**: Minimum SOL in bonding curve (e.g., 5)
   - **Max Liquidity SOL**: Maximum SOL in bonding curve (e.g., 100)
   - **Max SOL per Token**: Maximum price per token (e.g., 0.00001)
   - **Min Tokens Threshold**: Minimum token supply (e.g., 1000000)
6. Click **Save Settings**

### Step 2: Connect Wallet

1. Click **Connect Wallet** in header
2. Select your wallet (Phantom, Solflare, etc.)
3. Approve connection in wallet popup
4. Wallet address appears in header when connected

### Step 3: Start Monitoring

1. Click **Bot Control** panel
2. Select mode:
   - **Dry Run**: Simulate trades without spending (recommended for testing)
   - **Real Trading**: Execute actual trades (requires SOL in wallet)
3. Click **Start Bot**
4. Bot status changes to [RUNNING]

### Step 4: Monitor Detected Tokens

1. Go to **New Coins** tab
2. View detected tokens in real-time
3. Each token shows:
   - ✅ **Pass** (green) - Meets buy criteria
   - ❌ **Fail** (red) - Doesn't meet criteria
   - Evaluation reason (liquidity, price, etc.)
   - Token metadata (name, symbol, image)
   - Current price and liquidity
4. Filter by All / Pass / Fail

### Step 5: Buy Tokens

**Manual Buying**:
1. Find token marked ✅ Pass in New Coins
2. Ensure wallet is connected
3. Click **Buy Token** button
4. Approve transaction in wallet
5. Wait for confirmation (toast notification shown)
6. Holding automatically recorded

**Process**:
1. Transaction built by WASM bot
2. Wallet prompts for signature
3. Transaction submitted to Solana
4. Toast shows "Transaction Submitted" with Solscan link
5. Confirmation awaited
6. Toast shows "Transaction Confirmed"
7. Holding added to Holdings Panel

### Step 6: Monitor Holdings

1. Go to **Holdings** tab
2. View active positions:
   - Token name/symbol
   - Buy price in SOL
   - Token amount
   - Hold time (updating live)
3. **Sell Alerts**:
   - Yellow alert box appears when TP/SL/timeout triggers
   - Shows reason (TP, SL, or TIMEOUT)
   - Shows profit/loss percentage
   - Click **Sell Now** to execute
4. **Manual Selling**:
   - Click **Sell** button on any holding
   - Approve transaction in wallet
   - Wait for confirmation
   - Holding removed after successful sell

### Step 7: View Logs

1. Go to **Logs** tab
2. View bot activity:
   - Token detections
   - Evaluation results
   - Transaction submissions
   - Confirmations
   - Errors and warnings
3. Filter by log level (All / Info / Warn / Error)
4. Clear logs with Clear button

## 📱 User Interface Guide

### Header
- **Logo**: Sol Beast branding
- **Connect Wallet**: Wallet connection button
- **Theme Switcher**: Toggle dark/light theme
- **Status Indicators**: Connection status, bot state, mode

### Tabs

#### 1. Dashboard
- Overview of bot performance
- Stats: Total trades, win rate, P&L
- Charts: Performance over time
- Quick actions

#### 2. New Coins
- Real-time detected tokens
- Evaluation results (Pass/Fail)
- Token metadata and prices
- Buy buttons for qualifying tokens
- Filters: All / Pass / Fail

#### 3. Holdings
- Active positions list
- Buy price, amount, hold time
- Sell alerts (TP/SL/timeout)
- Manual and automatic sell buttons
- P&L tracking

#### 4. Trading History
- ⏳ Coming soon
- Completed trades
- P&L per trade
- Export to CSV

#### 5. Logs
- Bot activity log
- Filterable by level
- Detailed information
- Clear logs button

#### 6. Configuration
- Bot settings
- RPC/WebSocket endpoints
- Buy/sell parameters
- Heuristics configuration
- Save/reset buttons

## 🎨 Toast Notifications

Modern, non-blocking notifications for all bot actions:

### Success Toasts (Green)
- "Transaction Confirmed!"
- "Token purchased successfully"
- "Holding removed successfully"

### Error Toasts (Red)
- "Wallet Not Connected"
- "Failed to buy token"
- "Failed to sell token"

### Info Toasts (Blue)
- Settings saved
- Bot started/stopped

### Loading Toasts (Purple)
- "Building transaction..."
- "Confirming transaction..."

### Transaction Toasts (Interactive)
- Shows transaction signature
- "View on Solscan" button (clickable)
- Separate for submitted vs confirmed
- Auto-dismissible after 6-8 seconds

## ⚙️ Settings Reference

### Connection Settings
- **Solana WebSocket URLs**: Array of WebSocket endpoints for monitoring
  - Must support browser connections (CORS enabled)
  - Example: `["wss://mainnet.helius-rpc.com/?api-key=xxx"]`
- **Solana RPC URLs**: Array of HTTP RPC endpoints for transactions
  - Example: `["https://mainnet.helius-rpc.com/?api-key=xxx"]`

### Trading Settings
- **Buy Amount** (SOL): Amount to spend per token purchase
  - Default: 0.01
  - Range: 0.001 - 1.0
- **Slippage BPS**: Acceptable price slippage in basis points
  - Default: 500 (5%)
  - Range: 100 (1%) - 2000 (20%)
- **Take Profit %**: Profit target to trigger sell
  - Default: 100 (2x = 100% gain)
  - Range: 10 - 1000
- **Stop Loss %**: Maximum loss before selling
  - Default: -50 (-50% loss)
  - Range: -90 - -5
- **Timeout Seconds**: Maximum hold time before selling
  - Default: 300 (5 minutes)
  - Range: 60 - 3600

### Heuristics Settings
- **Min Liquidity SOL**: Minimum bonding curve liquidity
  - Default: 5
  - Purpose: Avoid low-liquidity scams
- **Max Liquidity SOL**: Maximum bonding curve liquidity
  - Default: 100
  - Purpose: Avoid overvalued tokens
- **Max SOL per Token**: Maximum acceptable price per token
  - Default: 0.00001
  - Purpose: Avoid overpriced tokens
- **Min Tokens Threshold**: Minimum token supply
  - Default: 1000000
  - Purpose: Avoid low-supply manipulation
- **Enable Safer Sniping**: Apply additional safety checks
  - Default: true
  - When true: More conservative buy criteria

### Program IDs
- **Pump Fun Program**: Pump.fun program address
  - Default: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`
- **Metadata Program**: Metaplex metadata program
  - Default: `metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`

## 🔧 Troubleshooting

### WebSocket Connection Fails
**Symptoms**: Bot shows [OFFLINE], no tokens detected

**Causes**:
1. Invalid RPC endpoint
2. Missing API key
3. Endpoint doesn't support browser WebSockets
4. CORS not enabled

**Solutions**:
- Use Helius, QuickNode, or Alchemy with API key
- Format: `wss://mainnet.helius-rpc.com/?api-key=YOUR_KEY`
- Test endpoint in browser console
- Check provider documentation

### Wallet Won't Connect
**Symptoms**: Wallet button doesn't work, no popup

**Causes**:
1. Wallet extension not installed
2. Wrong network (not Solana mainnet)
3. Wallet locked

**Solutions**:
- Install Phantom or Solflare extension
- Unlock wallet
- Switch to Solana mainnet in wallet settings
- Refresh page

### Buy Button Disabled
**Symptoms**: Buy button is grayed out

**Causes**:
1. Wallet not connected
2. Token doesn't pass evaluation
3. Bot not running

**Solutions**:
- Connect wallet first
- Check token evaluation reason
- Start bot if stopped

### Transaction Fails
**Symptoms**: Error toast after signing

**Causes**:
1. Insufficient SOL balance
2. Slippage too low
3. Token bonding curve changed
4. Network congestion

**Solutions**:
- Ensure sufficient SOL in wallet (buy amount + fees)
- Increase slippage to 8-10%
- Try again with updated price
- Wait for lower network load

### Holdings Not Updating
**Symptoms**: Holdings list doesn't refresh

**Causes**:
1. localStorage quota exceeded
2. Browser privacy mode
3. Page not refreshed after buy

**Solutions**:
- Refresh page to reload holdings
- Clear old holdings if list is too large
- Use normal browser mode (not incognito)

### Sell Alert Not Triggering
**Symptoms**: Price met TP but no alert

**Causes**:
1. Monitoring interval not reached
2. TP/SL settings too extreme
3. Bot not running

**Solutions**:
- Wait 10 seconds for next monitoring cycle
- Check TP/SL % settings are reasonable
- Ensure bot is running

## 🔐 Security Best Practices

1. **Never share private keys**
   - WASM mode uses browser wallets only
   - Never enter private keys in web interface

2. **Use hardware wallets**
   - Ledger supported for maximum security
   - Keeps private keys offline

3. **Verify transactions**
   - Always review transaction details in wallet
   - Check recipient address
   - Verify SOL amount

4. **Test with Dry Run mode**
   - Practice with dry run first
   - Switch to real trading only when confident

5. **Start with small amounts**
   - Begin with 0.001-0.01 SOL per trade
   - Increase gradually as you gain experience

6. **Use reputable RPC providers**
   - Stick to known providers (Helius, QuickNode, Alchemy)
   - Avoid unknown/unverified endpoints

7. **Keep wallet secure**
   - Lock wallet when not in use
   - Use strong password
   - Enable 2FA if available

## 📊 Performance Tips

1. **Optimize Polling**
   - Default: 2s for tokens, 5s for holdings, 10s for monitoring
   - Acceptable range: Don't go below 1s to avoid rate limits

2. **Manage Holdings**
   - Keep holdings list under 20 active positions
   - Clear old trades periodically
   - Holdings stored in localStorage (limited space)

3. **RPC Selection**
   - Use geographically close RPC endpoint
   - Paid tiers offer better performance
   - Multiple endpoints for redundancy

4. **Browser Performance**
   - Close unused tabs
   - Disable unnecessary extensions
   - Use modern browser (Chrome, Firefox, Edge)
   - Keep browser updated

## 🎯 Trading Strategies

### Conservative (Safer Sniping)
```
Buy Amount: 0.01 SOL
Min Liquidity: 10 SOL
Max Liquidity: 50 SOL
Max SOL per Token: 0.00001
Take Profit: 50% (1.5x)
Stop Loss: -30%
Timeout: 600s (10 min)
Enable Safer Sniping: true
```

### Moderate
```
Buy Amount: 0.05 SOL
Min Liquidity: 5 SOL
Max Liquidity: 100 SOL
Max SOL per Token: 0.0001
Take Profit: 100% (2x)
Stop Loss: -50%
Timeout: 300s (5 min)
Enable Safer Sniping: true
```

### Aggressive
```
Buy Amount: 0.1 SOL
Min Liquidity: 2 SOL
Max Liquidity: 200 SOL
Max SOL per Token: 0.001
Take Profit: 200% (3x)
Stop Loss: -70%
Timeout: 180s (3 min)
Enable Safer Sniping: false
```

## 📚 Additional Resources

- **GitHub Repository**: [MSB0095/sol_beast](https://github.com/MSB0095/sol_beast)
- **Documentation**: `WASM_MODE_STATUS.md`, `PHASE_5_SUMMARY.md`
- **Architecture**: `README.md` - Dual-mode architecture
- **API Reference**: `sol_beast_docs/` directory

## 🆘 Support

For issues, questions, or feature requests:
1. Check troubleshooting section above
2. Review WASM_MODE_STATUS.md for known limitations
3. Open GitHub issue with details:
   - Browser and version
   - Wallet used
   - Settings configuration
   - Error messages
   - Steps to reproduce

---

*Version: 1.0 (Phase 5.1)*  
*Last Updated: December 3, 2025*  
*WASM Mode Completion: ~92%*
