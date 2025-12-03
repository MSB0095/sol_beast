# Settings Reference

Complete reference for all Sol Beast configuration settings. Every parameter is explained with examples, defaults, and recommendations.

## 📋 Table of Contents

- [Network Settings](#network-settings)
- [Trading Mode](#trading-mode)
- [Buy Settings](#buy-settings)
- [Portfolio Limits](#portfolio-limits)
- [Heuristics & Filters](#heuristics--filters)
- [Position Management](#position-management)
- [Helius Sender](#helius-sender)
- [Logging](#logging)

---

## Network Settings

### `solana_rpc_urls`
**Type:** Array of Strings  
**Required:** Yes  
**Default:** None  

Array of Solana RPC endpoints for blockchain interaction. The bot will use these endpoints in rotation for reliability.

**Example:**
```toml
solana_rpc_urls = [
    "https://api.mainnet-beta.solana.com",
    "https://rpc.helius.xyz/?api-key=YOUR_KEY"
]
```

**Recommendations:**
- Use multiple endpoints for redundancy
- Premium RPC providers offer better performance
- WASM mode requires CORS-enabled endpoints
- Recommended providers:
  - [Helius](https://helius.dev) - Excellent for high-frequency trading
  - [QuickNode](https://quicknode.com) - Reliable and fast
  - [Triton](https://triton.one) - Good for WebSocket reliability

---

### `solana_ws_url`
**Type:** String  
**Required:** Yes  
**Default:** None

WebSocket endpoint for real-time blockchain event monitoring.

**Example:**
```toml
solana_ws_url = "wss://api.mainnet-beta.solana.com"
```

**Important:**
- Must support account subscriptions
- WASM mode uses this for browser-based connections
- Backend mode uses native WebSocket client

---

### `pump_program_id`
**Type:** String (Base58)  
**Required:** Yes  
**Default:** `"6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"`

Pump.fun program ID to monitor for token launch events.

**Example:**
```toml
pump_program_id = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
```

**Note:** This is the standard pump.fun program ID. Only change if monitoring a different program.

---

## Trading Mode

### `real_mode`
**Type:** Boolean  
**Required:** No  
**Default:** `false`

Controls whether the bot executes real transactions or simulates them.

**Values:**
- `false` - **Dry Run Mode** (SAFE) - Simulates all trades
- `true` - **Real Mode** (LIVE) - Executes actual transactions

**Example:**
```toml
real_mode = false  # Start with dry run!
```

**⚠️ WARNING:** 
- Always test with `real_mode = false` first
- Ensure you understand all settings before enabling
- Real mode will spend actual SOL
- Cannot be undone once transactions are submitted

---

## Buy Settings

### `max_sol_per_buy`
**Type:** Float  
**Required:** No  
**Default:** `0.1`  
**Units:** SOL

Maximum amount of SOL to spend on a single token purchase.

**Example:**
```toml
max_sol_per_buy = 0.05  # Spend max 0.05 SOL per buy
```

**Recommendations:**
- Start small (0.01-0.05 SOL) for testing
- Conservative: 0.1-0.5 SOL
- Aggressive: 1-5 SOL
- Consider token volatility and your risk tolerance

---

### `slippage_bps`
**Type:** Integer  
**Required:** No  
**Default:** `500`  
**Units:** Basis Points (0.01%)

Maximum acceptable slippage for trades.

**Example:**
```toml
slippage_bps = 300  # 3% slippage tolerance
```

**Conversions:**
- 100 bps = 1%
- 300 bps = 3%
- 500 bps = 5% (default)
- 1000 bps = 10%

**Recommendations:**
- Low liquidity: 500-1000 bps (5-10%)
- Medium liquidity: 300-500 bps (3-5%)
- High liquidity: 100-300 bps (1-3%)

---

## Portfolio Limits

### `max_portfolio_sol`
**Type:** Float  
**Required:** No  
**Default:** `10.0`  
**Units:** SOL

Maximum total SOL to allocate across all positions.

**Example:**
```toml
max_portfolio_sol = 1.0  # Max 1 SOL in total positions
```

**Important:**
- Bot won't buy if total allocation exceeds this limit
- Includes all open positions
- Does not include SOL reserved for gas fees

**Recommendations:**
- Conservative: 1-5 SOL
- Moderate: 5-20 SOL
- Aggressive: 20+ SOL
- Never allocate more than you can afford to lose

---

### `max_positions`
**Type:** Integer  
**Required:** No  
**Default:** `10`

Maximum number of simultaneous token positions.

**Example:**
```toml
max_positions = 5  # Hold max 5 tokens at once
```

**Considerations:**
- More positions = more diversification
- Fewer positions = easier to monitor
- Each position needs attention for exit management

**Recommendations:**
- Beginners: 3-5 positions
- Intermediate: 5-10 positions
- Advanced: 10-20 positions

---

## Heuristics & Filters

These settings control which tokens the bot will consider for purchase.

### `min_market_cap_sol`
**Type:** Float  
**Required:** No  
**Default:** `1000.0`  
**Units:** SOL

Minimum market cap (in SOL) for a token to be considered.

**Example:**
```toml
min_market_cap_sol = 5000.0  # ~$1M at $200/SOL
```

**Calculating USD Value:**
```
USD Value = SOL × SOL Price
Example: 5000 SOL × $200 = $1,000,000
```

**Recommendations:**
- Ultra-early: 100-1000 SOL ($20k-$200k)
- Early: 1000-5000 SOL ($200k-$1M)
- Established: 5000+ SOL ($1M+)

---

### `max_market_cap_sol`
**Type:** Float  
**Required:** No  
**Default:** `100000.0`  
**Units:** SOL

Maximum market cap (in SOL) for a token to be considered.

**Example:**
```toml
max_market_cap_sol = 50000.0  # ~$10M at $200/SOL
```

**Purpose:**
- Avoid tokens that have already "made it"
- Target early-stage opportunities
- Filter out too-expensive entries

---

### `min_holders`
**Type:** Integer  
**Required:** No  
**Default:** `0`

Minimum number of token holders required.

**Example:**
```toml
min_holders = 100  # Require at least 100 holders
```

**Purpose:**
- Ensure some token distribution
- Avoid extremely concentrated holdings
- Indicate genuine interest

**Recommendations:**
- Ultra-early: 0-50 holders
- Early: 50-200 holders
- Established: 200+ holders

---

### `max_dev_token_percentage`
**Type:** Float  
**Required:** No  
**Default:** `30.0`  
**Units:** Percentage (0-100)

Maximum percentage of tokens the developer can hold.

**Example:**
```toml
max_dev_token_percentage = 15.0  # Dev can hold max 15%
```

**Purpose:**
- Avoid rug pulls
- Ensure fair distribution
- Reduce centralization risk

**Recommendations:**
- Conservative: 5-10%
- Moderate: 10-20%
- Aggressive: 20-30%

**⚠️ Warning:** High developer holdings increase rug pull risk!

---

### `min_initial_buy_sol`
**Type:** Float  
**Required:** No  
**Default:** `0.0`  
**Units:** SOL

Minimum size of the first/initial buy to consider token legitimate.

**Example:**
```toml
min_initial_buy_sol = 0.1  # First buy must be at least 0.1 SOL
```

**Purpose:**
- Filter out test tokens
- Ensure genuine interest
- Avoid spam tokens

---

### `enable_social_filters`
**Type:** Boolean  
**Required:** No  
**Default:** `false`

Enable filtering based on social media presence (Twitter, Telegram, etc.).

**Example:**
```toml
enable_social_filters = true
```

**Requirements:**
- Token metadata must include social links
- Links must be valid and accessible

**Note:** This feature requires metadata fetching to be working properly.

---

## Position Management

### `take_profit_percentage`
**Type:** Float  
**Required:** No  
**Default:** `50.0`  
**Units:** Percentage

Profit target - sell when position gains this percentage.

**Example:**
```toml
take_profit_percentage = 100.0  # Sell at 2x (100% profit)
```

**Common Targets:**
- Conservative: 25-50% (1.25x-1.5x)
- Moderate: 50-100% (1.5x-2x)
- Aggressive: 100-300% (2x-4x)
- Moon: 500-1000% (6x-11x)

**Calculation:**
```
Sell Price = Buy Price × (1 + TP% / 100)
Example: $100 buy × (1 + 0.50) = $150 sell
```

---

### `stop_loss_percentage`
**Type:** Float (negative)  
**Required:** No  
**Default:** `-30.0`  
**Units:** Percentage

Loss limit - sell when position loses this percentage.

**Example:**
```toml
stop_loss_percentage = -20.0  # Sell at -20% loss
```

**Common Levels:**
- Tight: -10% to -20%
- Moderate: -20% to -40%
- Loose: -40% to -60%

**⚠️ Important:**
- Always use negative values
- Tighter stops = more frequent exits
- Looser stops = ride through volatility
- Consider gas fees when setting

---

### `timeout_minutes`
**Type:** Integer  
**Required:** No  
**Default:** `60`  
**Units:** Minutes

Maximum time to hold a position before auto-selling.

**Example:**
```toml
timeout_minutes = 30  # Exit after 30 minutes
```

**Purpose:**
- Prevent capital being locked
- Force reallocation of dead positions
- Manage opportunity cost

**Recommendations:**
- Scalping: 5-15 minutes
- Day trading: 15-60 minutes
- Swing: 60-240 minutes
- Long-term: Disable (set very high)

---

### `trailing_stop_percentage`
**Type:** Float  
**Required:** No  
**Default:** `null` (disabled)  
**Units:** Percentage

Enable trailing stop - lock in profits as price rises.

**Example:**
```toml
trailing_stop_percentage = 10.0  # Trail 10% below peak
```

**How It Works:**
1. Position reaches new high
2. Stop loss moves up to: High × (1 - Trail%)
3. If price drops by Trail%, sell

**Example:**
```
Buy: $100
Peak: $200
Trailing Stop: $200 × 0.90 = $180
If price drops to $180, sell for 80% profit
```

---

## Helius Sender

Integration with Helius Sender for ultra-low latency transaction submission.

### `helius_sender_enabled`
**Type:** Boolean  
**Required:** No  
**Default:** `false`

Enable Helius Sender for transaction submission.

**Example:**
```toml
helius_sender_enabled = true
```

**Benefits:**
- Faster transaction confirmation
- Dual routing to validators + Jito
- Dynamic priority fees
- Automatic compute optimization

**Requirements:**
- Helius account (free tier available)
- SOL for tips (minimum 0.001 SOL per transaction)

See [Helius Integration](../helius/overview.md) for complete documentation.

---

### `helius_sender_endpoint`
**Type:** String  
**Required:** If `helius_sender_enabled = true`  
**Default:** `"https://sender.helius-rpc.com/fast"`

Helius Sender endpoint URL.

**Example:**
```toml
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
```

**Options:**
- Global HTTPS: `https://sender.helius-rpc.com/fast`
- Regional (backend only):
  - Salt Lake City: `http://slc-sender.helius-rpc.com/fast`
  - Newark: `http://ewr-sender.helius-rpc.com/fast`
  - London: `http://lon-sender.helius-rpc.com/fast`
  - Frankfurt: `http://fra-sender.helius-rpc.com/fast`

---

### `helius_min_tip_sol`
**Type:** Float  
**Required:** No  
**Default:** `0.001`  
**Units:** SOL

Minimum tip amount for Helius Sender transactions.

**Example:**
```toml
helius_min_tip_sol = 0.005  # Tip 0.005 SOL per transaction
```

**Cost Impact:**
```
100 transactions × 0.001 SOL = 0.1 SOL (~$20 at $200/SOL)
100 transactions × 0.005 SOL = 0.5 SOL (~$100 at $200/SOL)
```

**Routing Mode Minimums:**
- Dual routing: 0.001 SOL
- SWQOS-only: 0.000005 SOL (200x cheaper!)

---

### `helius_use_dynamic_tips`
**Type:** Boolean  
**Required:** No  
**Default:** `true`

Automatically fetch and use dynamic tip amounts from Jito API.

**Example:**
```toml
helius_use_dynamic_tips = true
```

**How It Works:**
- Queries Jito tip floor API
- Uses 75th percentile of recent tips
- Falls back to `helius_min_tip_sol` if API unavailable
- Adjusts to network competition

**Recommendations:**
- Enable for competitive launches
- Disable for cost savings during quiet periods

---

### `helius_priority_fee_multiplier`
**Type:** Float  
**Required:** No  
**Default:** `1.2`

Multiplier for Helius-recommended priority fees.

**Example:**
```toml
helius_priority_fee_multiplier = 1.5  # 50% above recommended
```

**Purpose:**
- Increase transaction priority
- Improve confirmation speed
- Outbid competitors during congestion

**Recommendations:**
- Normal: 1.0-1.2
- Competitive: 1.2-1.5
- Ultra-competitive: 1.5-2.0

---

## Logging

### `log_level`
**Type:** String  
**Required:** No  
**Default:** `"info"`  

Controls log verbosity.

**Example:**
```toml
log_level = "debug"
```

**Levels (least to most verbose):**
- `error` - Only errors
- `warn` - Errors and warnings
- `info` - General information (recommended)
- `debug` - Detailed debugging information
- `trace` - Very verbose, includes all events

**Environment Variable:**
```bash
RUST_LOG=info cargo run
RUST_LOG=debug,sol_beast=trace cargo run  # Component-specific
```

---

## Complete Example Configuration

Here's a complete, production-ready configuration:

```toml
# Network
solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]
solana_ws_url = "wss://api.mainnet-beta.solana.com"
pump_program_id = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"

# Trading Mode
real_mode = false  # Start with dry run!

# Buy Settings
max_sol_per_buy = 0.1
slippage_bps = 500

# Portfolio
max_portfolio_sol = 2.0
max_positions = 5

# Heuristics
min_market_cap_sol = 5000.0
max_market_cap_sol = 50000.0
min_holders = 50
max_dev_token_percentage = 20.0
min_initial_buy_sol = 0.05

# Position Management
take_profit_percentage = 100.0
stop_loss_percentage = -30.0
timeout_minutes = 60

# Helius Sender
helius_sender_enabled = true
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
helius_min_tip_sol = 0.001
helius_use_dynamic_tips = true
helius_priority_fee_multiplier = 1.2

# Logging
log_level = "info"
```

---

## Next Steps

- **[Network Settings](./network-settings.md)** - Detailed RPC configuration
- **[Trading Settings](./trading-settings.md)** - Advanced trading options
- **[Risk Management](./risk-management.md)** - Protecting your capital
- **[Heuristics Guide](./heuristics.md)** - Token filtering strategies
