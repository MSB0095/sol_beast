# Configuration Guide

This guide covers all configuration options available in `config.toml`.

## Configuration File Structure

```toml
# Basic Settings
wallet_keypair_path = "/path/to/keypair.json"
solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]

# Trading Parameters
buy_amount_sol = 0.05
max_slippage_bps = 500
take_profit_percentage = 50.0
stop_loss_percentage = 20.0
timeout_seconds = 300

# Heuristic Filters
min_token_age_seconds = 60
min_holder_count = 50
min_liquidity_sol = 10.0

# Helius Sender (Optional)
helius_sender_enabled = false
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
helius_min_tip_sol = 0.001
helius_priority_fee_multiplier = 1.2
helius_use_dynamic_tips = true
helius_use_swqos_only = false
```

## Basic Settings

### `wallet_keypair_path`
- **Type**: String (file path)
- **Required**: Yes (for --real mode)
- **Example**: `"/home/user/.config/solana/trading-wallet.json"`
- **Description**: Path to your Solana wallet keypair JSON file

::: danger Security
- Use a dedicated trading wallet
- Never commit keypair files to git
- Set file permissions to 600 (`chmod 600 keypair.json`)
- Only fund with amounts you can afford to lose
:::

### `solana_rpc_urls`
- **Type**: Array of strings
- **Required**: Yes
- **Example**: `["https://rpc.helius.xyz/?api-key=YOUR_KEY"]`
- **Description**: List of Solana RPC endpoints (bot will use first available)

**Recommended RPC Providers:**
- [Helius](https://helius.dev/) - Free tier available, excellent performance
- [QuickNode](https://www.quicknode.com/) - Reliable, paid tiers
- [Triton](https://triton.one/) - High-performance options
- Public RPC - Free but slower

## Trading Parameters

### `buy_amount_sol`
- **Type**: Float
- **Default**: `0.05`
- **Range**: `0.001` - `10.0` (adjust based on capital)
- **Description**: Amount of SOL to spend per trade

**Recommendations:**
- Start small: `0.01-0.05 SOL`
- Scale up gradually as confidence grows
- Consider gas fees and tips (add ~0.001 SOL buffer)

### `max_slippage_bps`
- **Type**: Integer (basis points)
- **Default**: `500` (5%)
- **Range**: `50` (0.5%) - `2000` (20%)
- **Description**: Maximum acceptable slippage

**Common Values:**
- Conservative: `200-300` (2-3%)
- Moderate: `500` (5%)
- Aggressive: `1000+` (10%+)

::: warning High Slippage
Higher slippage = more trades execute but at worse prices. Find the balance for your strategy.
:::

### `take_profit_percentage`
- **Type**: Float
- **Default**: `50.0` (50% profit)
- **Range**: `5.0` - `500.0`
- **Description**: Sell when price increases by this percentage

**Strategy Examples:**
- Scalping: `5-15%`
- Swing: `30-100%`
- HODL: `200-500%`

### `stop_loss_percentage`
- **Type**: Float
- **Default**: `20.0` (20% loss)
- **Range**: `5.0` - `90.0`
- **Description**: Sell when price decreases by this percentage

**Risk Profiles:**
- Conservative: `10-15%`
- Moderate: `20-30%`
- Aggressive: `40-50%`

::: tip Risk Management
Never risk more than 1-2% of your total capital per trade. Set stop-loss accordingly.
:::

### `timeout_seconds`
- **Type**: Integer (seconds)
- **Default**: `300` (5 minutes)
- **Range**: `60` - `3600`
- **Description**: Sell after this many seconds regardless of profit/loss

**Use Cases:**
- Quick flips: `60-180` seconds
- Medium hold: `300-600` seconds
- Patient: `900-1800` seconds

## Heuristic Filters

These filters determine which tokens the bot will consider buying.

### `min_token_age_seconds`
- **Type**: Integer (seconds)
- **Default**: `60`
- **Range**: `0` - `600`
- **Description**: Only buy tokens older than this

**Rationale:** Newer tokens = higher risk, potential for rug pulls

### `min_holder_count`
- **Type**: Integer
- **Default**: `50`
- **Range**: `10` - `500`
- **Description**: Only buy tokens with at least this many holders

**Rationale:** More holders = more liquidity and less manipulation risk

### `min_liquidity_sol`
- **Type**: Float (SOL)
- **Default**: `10.0`
- **Range**: `1.0` - `100.0`
- **Description**: Only buy tokens with at least this much liquidity

**Rationale:** Higher liquidity = easier to exit without major slippage

## Helius Sender Configuration

See [Helius Sender Guide](/guide/helius-sender) for detailed information.

### Quick Reference

```toml
# Enable ultra-low latency
helius_sender_enabled = true

# Choose routing mode
helius_use_swqos_only = false  # false = dual routing, true = SWQOS only

# Set minimum tip
helius_min_tip_sol = 0.001     # 0.001 for dual, 0.000005 for SWQOS

# Auto-adjust tips based on network
helius_use_dynamic_tips = true

# Priority fee multiplier
helius_priority_fee_multiplier = 1.2  # 1.2 = 20% above recommended
```

## Advanced Options

### API Server

```toml
api_host = "0.0.0.0"
api_port = 8080
```

### Logging

```toml
log_level = "info"  # debug, info, warn, error
```

Set via environment variable instead:
```bash
RUST_LOG=debug cargo run
```

## Example Configurations

### Conservative (Low Risk)

```toml
buy_amount_sol = 0.01
max_slippage_bps = 300
take_profit_percentage = 30.0
stop_loss_percentage = 15.0
timeout_seconds = 180
min_token_age_seconds = 120
min_holder_count = 100
min_liquidity_sol = 20.0
helius_sender_enabled = false
```

### Moderate (Balanced)

```toml
buy_amount_sol = 0.05
max_slippage_bps = 500
take_profit_percentage = 50.0
stop_loss_percentage = 20.0
timeout_seconds = 300
min_token_age_seconds = 60
min_holder_count = 50
min_liquidity_sol = 10.0
helius_sender_enabled = true
helius_use_swqos_only = true
```

### Aggressive (High Risk/Reward)

```toml
buy_amount_sol = 0.1
max_slippage_bps = 1000
take_profit_percentage = 100.0
stop_loss_percentage = 30.0
timeout_seconds = 600
min_token_age_seconds = 30
min_holder_count = 20
min_liquidity_sol = 5.0
helius_sender_enabled = true
helius_use_swqos_only = false
helius_use_dynamic_tips = true
```

## Configuration Best Practices

1. **Start Conservative**: Use low amounts and tight stop-losses
2. **Test in Dry Mode**: Always test configuration changes first
3. **Monitor Performance**: Track which settings work best
4. **Adjust Gradually**: Make small incremental changes
5. **Document Changes**: Keep notes on what settings you try

---

::: tip Next Steps
- Learn about [Trading Parameters](/guide/trading-parameters) in detail
- Configure [Helius Sender](/guide/helius-sender) for speed
- Understand [Risk Management](/advanced/risk-management)
:::
