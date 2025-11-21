# sol_beast Setup Guide

This guide walks you through setting up sol_beast with proper licensing and dev fee configuration.

## Prerequisites

Before you begin, you need:

1. ✅ A valid **license key** (contact developer)
2. ✅ A Solana wallet for **dev fee collection**
3. ✅ A Solana wallet for **trading** (for --real mode)
4. ✅ Rust toolchain installed (`cargo` command available)

---

## Step 1: Obtain Your License Key

Contact the sol_beast developer to obtain your license key. You'll receive a unique Base58-encoded key that looks like this:

```
7xKx9L2YzH8aB4cD5eF6gH7iJ8kL9mN0pQ1rS2tU3vW4xY5zA6bC7dE8fG9hI0jK
```

Keep this key confidential - it's unique to your deployment.

---

## Step 2: Configure Your Settings

### 2.1 Copy Example Configuration

```bash
cd /path/to/sol_beast
cp config.example.toml config.toml
```

### 2.2 Edit Configuration File

Open `config.toml` in your text editor and configure the following **required** fields:

#### A. License Key (REQUIRED)

```toml
# REQUIRED: Your unique license key
license_key = "YOUR_LICENSE_KEY_HERE"
```

Replace `YOUR_LICENSE_KEY_HERE` with the license key you received from the developer.

#### B. Dev Fee Wallet (REQUIRED)

```toml
# REQUIRED: Your Solana wallet address for receiving dev fees
dev_fee_wallet = "YOUR_SOLANA_WALLET_ADDRESS_HERE"
```

**What is this?**
- This is YOUR wallet address that will receive the 2% dev fee from YOUR trades
- Use your own wallet address - the fee comes from your trades to support development
- Example: `"7xKx9L2YzH8aB4cD5eF6gH7iJ8kL9mN0pQ1rS2tU3vW4xY5zA6bC7dE8fG9hI0jK"`

**Important:** This is NOT the developer's wallet - it's YOUR wallet for YOUR instance.

#### C. Dev Fee Percentage (DO NOT MODIFY)

```toml
# Standard 2% dev fee (DO NOT MODIFY)
dev_fee_bps = 200
```

The dev fee is set at 2% (200 basis points). **Do not modify this value** as it violates the license agreement.

#### D. Trading Wallet (for --real mode)

Choose ONE of the following methods:

**Option 1: Keypair File (Simple)**
```toml
wallet_keypair_path = "./keypair.json"
```

**Option 2: Base58 Private Key (Recommended)**
```toml
wallet_private_key_string = "YOUR_BASE58_PRIVATE_KEY"
```

**Option 3: Environment Variable (Most Secure)**
```bash
# Create base64-encoded keypair
export SOL_BEAST_KEYPAIR_B64=$(cat keypair.json | base64 -w0)
```

Then run sol_beast - it will automatically use the environment variable.

---

## Step 3: Configure RPC/WSS Endpoints

Update these to your preferred Solana RPC providers:

```toml
solana_rpc_urls = ["https://api.mainnet-beta.solana.com/"]
solana_ws_urls = ["wss://api.mainnet-beta.solana.com/"]
```

**Recommended providers:**
- Helius (https://helius.dev)
- QuickNode (https://quicknode.com)
- Alchemy (https://alchemy.com)
- Triton (https://triton.one)

---

## Step 4: Verify Your Configuration

Before running in real mode, verify your setup:

```bash
# Test configuration loading
RUST_LOG=info cargo run -- --help
```

You should see the license banner:

```
╔═══════════════════════════════════════════════════════════════╗
║                        sol_beast v0.1.0                       ║
║                 Licensed Software - All Rights Reserved       ║
╚═══════════════════════════════════════════════════════════════╝

✓ License key validated successfully
License validated: Standard perpetual license
```

If you see errors about missing license key or invalid format, review Step 2.

---

## Step 5: Run in Dry-Run Mode (Safe Testing)

Before trading with real funds, test in dry-run mode:

```bash
RUST_LOG=info cargo run
```

In this mode:
- ✅ License validation occurs
- ✅ Transactions are simulated (not sent)
- ✅ No real SOL is spent
- ✅ You can verify the bot logic works
- ❌ Dev fees are not actually charged (simulation only)

Monitor the logs to ensure:
- License validates successfully
- Bot detects new tokens
- Simulated buys/sells execute without errors

---

## Step 6: Run in Real Mode (Actual Trading)

**⚠️ WARNING: Real mode uses actual SOL and charges real dev fees**

Only proceed when you:
1. ✅ Have tested thoroughly in dry-run mode
2. ✅ Understand you will pay 2% dev fee on every trade
3. ✅ Have configured your trading wallet with sufficient SOL
4. ✅ Accept the risks of automated trading

```bash
RUST_LOG=info cargo run --release -- --real
```

In this mode:
- 💰 Real SOL is spent on buys
- 💰 Real SOL is received on sells
- 💰 2% dev fee is charged on every transaction
- ⚠️ Losses are permanent - trade responsibly

---

## Understanding the Dev Fee

### How It Works

**On Buy:**
```
You buy with: 1.0 SOL
Bot spends:   1.0 SOL on tokens + 0.02 SOL dev fee = 1.02 SOL total
```

**On Sell:**
```
You receive:  1.5 SOL from sale
Bot sends:    1.5 SOL to you + 0.03 SOL dev fee = 1.53 SOL total moved
```

The dev fee is **automatically added** to every transaction. You'll see it in the logs:

```
INFO Added dev fee: 0.020000 SOL (200 basis points) to 7xKx9...
```

### Where Does the Fee Go?

- 💰 **YOUR wallet receives the dev fee** from YOUR trades
- 🔧 The fee supports ongoing development, updates, and maintenance
- 📚 Ensures you receive support, documentation, and new features
- 🔒 Validates your license is authentic and active

### Can I Disable It?

**NO.** The 2% dev fee is:
- ✅ Non-negotiable per license agreement
- ✅ Hardcoded in the transaction logic
- ✅ Required for license compliance
- ❌ Cannot be removed or modified

Attempting to bypass the dev fee violates the license and may result in:
- License revocation
- Loss of support and updates
- Legal action

---

## Troubleshooting

### "No license key found in config.toml"

**Solution:** Add `license_key = "YOUR_KEY"` to config.toml

### "Invalid license key: too short"

**Solution:** Ensure you copied the entire license key (minimum 32 characters)

### "Invalid license key: checksum verification failed"

**Solution:** 
- Check for typos in the license key
- Ensure you copied it correctly
- Contact developer if the key is corrupted

### "License key expired"

**Solution:** For time-limited licenses, contact developer to renew

### "Invalid dev_fee_wallet address"

**Solution:** 
- Ensure the wallet address is valid Solana base58 format
- Check for typos or extra spaces
- Example: `"7xKx9L2YzH8aB4cD5eF6gH7iJ8kL9mN0pQ1rS2tU3vW4xY5zA6bC7dE8fG9hI0jK"`

### Bot starts but doesn't execute trades

**Possible causes:**
1. Not enough SOL in trading wallet
2. RPC/WSS endpoints are rate-limited or down
3. No new pump.fun tokens meeting your criteria
4. Max held coins limit reached

Check logs with `RUST_LOG=debug cargo run` for detailed diagnostics.

---

## Security Best Practices

1. 🔒 **Never share your license key** - it's unique to your deployment
2. 🔒 **Never commit private keys** to git repositories
3. 🔒 **Use environment variables** for sensitive data in production
4. 🔒 **Keep config.toml** out of version control (add to .gitignore)
5. 🔒 **Start with small buy_amount** until you're confident in the bot
6. 🔒 **Monitor regularly** - automated trading has risks

---

## Example Complete Configuration

Here's a minimal working `config.toml`:

```toml
# RPC & WebSocket
solana_ws_urls = ["wss://YOUR-RPC-PROVIDER.com/"]
solana_rpc_urls = ["https://YOUR-RPC-PROVIDER.com/"]

# License (REQUIRED)
license_key = "YOUR_LICENSE_KEY_FROM_DEVELOPER_HERE_MUST_BE_32_PLUS_CHARS"

# Dev Fee (REQUIRED)
dev_fee_wallet = "YOUR_SOLANA_WALLET_ADDRESS_FOR_RECEIVING_DEV_FEES_HERE"
dev_fee_bps = 200  # DO NOT MODIFY

# Trading Wallet
wallet_keypair_path = "./keypair.json"

# Programs (usually don't change)
pump_fun_program = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
metadata_program = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"

# Strategy
tp_percent = 30.0
sl_percent = -20.0
timeout_secs = 3600
buy_amount = 0.1
max_holded_coins = 100

# Safety Features
enable_safer_sniping = true
min_tokens_threshold = 1000000
max_sol_per_token = 0.0001
slippage_bps = 500
min_liquidity_sol = 0.0
max_liquidity_sol = 100.0

# Performance
price_source = "wss"
rotate_rpc = true
cache_capacity = 1024
price_cache_ttl_secs = 30
```

---

## Getting Help

**For licensed users:**
- 📧 Email support (see LICENSING.md)
- 🐛 GitHub Issues (for licensed users only)
- 💬 Community channels (Discord/Telegram)

**Include in support requests:**
- License key (if related to activation)
- Relevant log excerpts
- Configuration file (redact private keys!)
- Steps to reproduce the issue

---

## Next Steps

After completing setup:

1. ✅ Read [LICENSING.md](LICENSING.md) for complete terms
2. ✅ Review [README.md](README.md) for feature documentation
3. ✅ Join the community for tips and strategies
4. ✅ Start trading responsibly with small amounts
5. ✅ Monitor your results and adjust strategy

**Good luck with your trading! 🚀**

---

**Version:** 1.0  
**Last Updated:** January 2025
