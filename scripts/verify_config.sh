#!/bin/bash
# Configuration Verification Script for sol_beast
# Checks that all required settings are properly configured

set -e

CONFIG_FILE="${1:-config.toml}"

echo "╔═══════════════════════════════════════════════════════════════╗"
echo "║          sol_beast Configuration Verification Tool            ║"
echo "╚═══════════════════════════════════════════════════════════════╝"
echo ""

if [ ! -f "$CONFIG_FILE" ]; then
    echo "❌ ERROR: Configuration file not found: $CONFIG_FILE"
    echo ""
    echo "Create it with:"
    echo "  cp config.example.toml config.toml"
    echo ""
    exit 1
fi

echo "✓ Configuration file found: $CONFIG_FILE"
echo ""
echo "Checking required settings..."
echo ""

# Function to check if a setting exists and is not a placeholder
check_setting() {
    local key="$1"
    local description="$2"
    local placeholder="$3"
    
    if ! grep -q "^${key} *=" "$CONFIG_FILE"; then
        echo "❌ MISSING: $description"
        echo "   Add '$key = \"YOUR_VALUE\"' to $CONFIG_FILE"
        return 1
    fi
    
    local value=$(grep "^${key} *=" "$CONFIG_FILE" | head -1 | cut -d'=' -f2- | tr -d ' "')
    
    if [ -z "$value" ]; then
        echo "❌ EMPTY: $description"
        echo "   Set a value for '$key' in $CONFIG_FILE"
        return 1
    fi
    
    if [ -n "$placeholder" ] && echo "$value" | grep -qi "$placeholder"; then
        echo "❌ PLACEHOLDER: $description"
        echo "   Current: $key = $value"
        echo "   Replace the placeholder with your actual value"
        return 1
    fi
    
    echo "✓ $description"
    return 0
}

# Check all required settings
ERRORS=0

check_setting "license_key" "License Key" "REPLACE" || ERRORS=$((ERRORS + 1))
check_setting "dev_fee_wallet" "Dev Fee Wallet" "REPLACE" || ERRORS=$((ERRORS + 1))
check_setting "dev_fee_bps" "Dev Fee Basis Points" "" || ERRORS=$((ERRORS + 1))

echo ""
echo "Checking dev fee configuration..."
echo ""

# Verify dev_fee_bps is 200 (2%)
DEV_FEE=$(grep "^dev_fee_bps *=" "$CONFIG_FILE" | head -1 | cut -d'=' -f2 | tr -d ' ')
if [ "$DEV_FEE" != "200" ]; then
    echo "⚠️  WARNING: dev_fee_bps should be 200 (2%)"
    echo "   Current value: $DEV_FEE"
    echo "   Modifying this violates the license agreement"
    ERRORS=$((ERRORS + 1))
else
    echo "✓ Dev fee correctly set to 200 basis points (2%)"
fi

echo ""
echo "Checking license key format..."
echo ""

LICENSE_KEY=$(grep "^license_key *=" "$CONFIG_FILE" | head -1 | cut -d'=' -f2- | tr -d ' "')
if [ ${#LICENSE_KEY} -lt 32 ]; then
    echo "❌ ERROR: License key too short (${#LICENSE_KEY} chars, minimum 32)"
    echo "   Contact developer for a valid license key"
    ERRORS=$((ERRORS + 1))
else
    echo "✓ License key format looks valid (${#LICENSE_KEY} characters)"
fi

echo ""
echo "Checking wallet configuration..."
echo ""

HAS_WALLET=0
if grep -q "^wallet_keypair_path *=" "$CONFIG_FILE"; then
    WALLET_PATH=$(grep "^wallet_keypair_path *=" "$CONFIG_FILE" | head -1 | cut -d'=' -f2- | tr -d ' "')
    if [ -f "$WALLET_PATH" ]; then
        echo "✓ Wallet keypair file found: $WALLET_PATH"
        HAS_WALLET=1
    else
        echo "⚠️  Wallet keypair file not found: $WALLET_PATH"
    fi
elif grep -q "^wallet_private_key_string *=" "$CONFIG_FILE"; then
    echo "✓ Wallet private key string configured"
    HAS_WALLET=1
elif grep -q "^wallet_keypair_json *=" "$CONFIG_FILE"; then
    echo "✓ Wallet keypair JSON configured"
    HAS_WALLET=1
fi

if [ $HAS_WALLET -eq 0 ]; then
    echo "⚠️  No wallet configured (required for --real mode)"
    echo "   Set one of: wallet_keypair_path, wallet_private_key_string, or SOL_BEAST_KEYPAIR_B64"
fi

echo ""
echo "Checking RPC/WSS endpoints..."
echo ""

if ! grep -q "^solana_rpc_urls *=" "$CONFIG_FILE"; then
    echo "❌ ERROR: Missing solana_rpc_urls"
    ERRORS=$((ERRORS + 1))
else
    echo "✓ RPC URLs configured"
fi

if ! grep -q "^solana_ws_urls *=" "$CONFIG_FILE"; then
    echo "❌ ERROR: Missing solana_ws_urls"
    ERRORS=$((ERRORS + 1))
else
    echo "✓ WebSocket URLs configured"
fi

echo ""
echo "════════════════════════════════════════════════════════════════"

if [ $ERRORS -eq 0 ]; then
    echo "✅ Configuration verification PASSED"
    echo ""
    echo "Your configuration looks good!"
    echo ""
    echo "Next steps:"
    echo "  1. Test in dry-run mode: RUST_LOG=info cargo run"
    echo "  2. When ready: RUST_LOG=info cargo run --release -- --real"
    echo ""
    echo "Remember:"
    echo "  • 2% dev fee applies to all transactions"
    echo "  • Read LICENSING.md for complete terms"
    echo "  • Start with small buy_amount for testing"
    echo ""
    exit 0
else
    echo "❌ Configuration verification FAILED ($ERRORS errors)"
    echo ""
    echo "Fix the errors above before running sol_beast."
    echo "Refer to SETUP_GUIDE.md for detailed instructions."
    echo ""
    exit 1
fi
