# SolBeast Dev Fee Implementation Guide

## Overview

This document describes the 2% dev fee implementation with anti-copycat protection for the SolBeast trading bot.

## Architecture

The dev fee system consists of two main components:

### 1. Smart Contract (`program/`)

A minimal Solana smart contract (<500 bytes) that:
- Validates two obfuscated magic codes (XOR-encoded)
- Enforces dev wallet address
- Transfers 2% of transaction amount to dev wallet
- Supports both buy (0) and sell (1) operations

**Key Files:**
- `program/src/lib.rs` - Smart contract code
- `program/Cargo.toml` - Build configuration
- `program/README.md` - Deployment guide

### 2. Backend Integration (`src/dev_fee.rs`)

Rust module that:
- Generates magic codes for transactions
- Adds dev fee instructions to buy/sell transactions
- Validates dev fee presence before submission
- Calculates 2% fee based on transaction amount

## How It Works

### Transaction Flow

```
User Action (Buy/Sell)
    ↓
Backend creates transaction
    ↓
Add dev fee transfer instruction (2% to dev wallet)
    ↓
Add magic codes to instruction data
    ↓
Submit to Solana network
    ↓
Smart contract validates:
    - Magic codes correct?
    - Dev wallet correct?
    - Signature valid?
    ↓
Transfer 2% fee → Execute main operation
```

### Magic Code Protection

The system uses two 8-byte magic codes that are:
1. **Obfuscated**: XOR-encoded with 0x42 and 0x7F
2. **Embedded**: Hardcoded in smart contract bytecode
3. **Validated**: Every transaction must include them
4. **Backend-only**: Only the SolBeast backend knows how to generate them

Example:
```rust
// Magic Code 1 (after XOR with 0x42)
[0x73, 0x91, 0xC5, 0x28, 0x65, 0xF7, 0x2B, 0xD4]

// Magic Code 2 (after XOR with 0x7F)
[0x1E, 0x8C, 0x42, 0xD9, 0x57, 0x3A, 0x6F, 0xB2]
```

## Configuration

### Enable Dev Fee

In `config.toml`:
```toml
# Enable 2% dev fee on all transactions
dev_fee_enabled = true

# Optional: Override dev wallet (uses default if not specified)
# dev_wallet_address = "YourDevWalletAddressHere"
```

### Settings Structure

```rust
pub struct Settings {
    // ... other fields ...
    
    // Dev fee configuration
    pub dev_fee_enabled: bool,
    pub dev_wallet_address: Option<String>,
}
```

## Deployment Checklist

### 1. Configure Dev Wallet

Before deploying the smart contract, update `program/src/lib.rs`:

```rust
const DEV_WALLET: [u8; 32] = [
    // Replace with your actual dev wallet bytes
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1
];
```

### 2. Build Smart Contract

```bash
cd program
cargo build-sbf
```

Verify size:
```bash
ls -lh target/deploy/solbeast_dev_fee.so
```

Should be under 500 bytes.

### 3. Deploy to Solana

**Devnet (testing):**
```bash
solana config set --url devnet
solana program deploy target/deploy/solbeast_dev_fee.so
```

**Mainnet (production):**
```bash
solana config set --url mainnet-beta
solana program deploy target/deploy/solbeast_dev_fee.so
```

Save the Program ID returned.

### 4. Update Backend

Update `src/dev_fee.rs`:

```rust
// Replace with your deployed program ID
const DEV_FEE_PROGRAM_ID: &str = "YOUR_PROGRAM_ID_HERE";

// Replace with your dev wallet
const DEV_WALLET: &str = "YOUR_DEV_WALLET_HERE";
```

### 5. Configure and Test

1. Update `config.toml` with dev fee settings
2. Build backend: `cargo build --release`
3. Test in dry mode: `cargo run`
4. Test with small amounts: `cargo run --release -- --real`

## Security Considerations

### Why This Is Hard to Copy

1. **Obfuscated Magic Codes**: Attackers must reverse-engineer XOR encoding
2. **Hardcoded Addresses**: Dev wallet embedded in bytecode
3. **Backend Control**: Only official backend generates valid instruction data
4. **Closed Source Binary**: While source is visible, bytecode is obfuscated
5. **Byte-Level Validation**: Smart contract checks exact byte sequences

### What Attackers Would Need To Do

To create a zero-fee version:
1. Reverse engineer compiled bytecode
2. Identify XOR keys (0x42, 0x7F)
3. Extract magic code patterns
4. Modify smart contract source
5. Re-deploy with different program ID
6. Modify their entire backend to use new contract
7. Update all frontend integrations

This is significantly more work than paying the 2% fee.

### Additional Hardening (Optional)

For production, consider:
1. Regenerating magic codes with different XOR keys
2. Adding timestamp-based validation
3. Implementing rate limiting per wallet
4. Adding program upgrade authority restrictions

## Fee Calculation

The 2% fee is calculated as:
```rust
fee_amount = transaction_amount_lamports / 50
```

Examples:
- 1 SOL transaction → 0.02 SOL fee (20M lamports)
- 0.1 SOL transaction → 0.002 SOL fee (2M lamports)
- 0.01 SOL transaction → 0.0002 SOL fee (200K lamports)

## Monitoring

### Check Fee Collection

View dev wallet balance:
```bash
solana balance <DEV_WALLET_ADDRESS>
```

### Transaction Logs

The backend logs dev fee transactions:
```
INFO: Added 2% dev fee to buy transaction
INFO: Dev fee collected: 20000000 lamports (0.02 SOL) for op_type 0
```

### Failed Transactions

If magic codes are invalid or dev wallet doesn't match:
```
ERROR: Invalid magic code 1
ERROR: Invalid dev wallet
```

## Troubleshooting

### "Dev fee not found in instructions"

- Ensure `dev_fee_enabled = true` in config
- Check that backend is properly integrated
- Verify dev_wallet_address is set correctly

### "Invalid magic code"

- Magic codes must be generated by official backend
- Check that instruction data includes correct XOR-encoded values
- Verify smart contract deployment matches backend constants

### "Invalid dev wallet"

- Dev wallet in backend must match hardcoded contract address
- Verify DEV_WALLET constant in both contract and backend
- Check that wallet address is in correct format (bytes)

## Maintenance

### Updating Magic Codes

To regenerate magic codes for additional security:

1. Generate new random 8-byte sequences
2. Update `M1` and `M2` constants in contract
3. Update corresponding constants in `src/dev_fee.rs`
4. Rebuild and redeploy contract
5. Rebuild and redeploy backend

### Changing Dev Wallet

1. Update `DEV_WALLET` in `program/src/lib.rs`
2. Update `DEV_WALLET` in `src/dev_fee.rs`
3. Rebuild and redeploy contract
4. Rebuild and restart backend
5. Update `config.toml` if using override

## Performance Impact

- **Smart Contract**: <500 bytes, minimal on-chain footprint
- **Execution Cost**: ~0.000005 SOL per transaction (5,000 lamports)
- **Backend Overhead**: Negligible (<1ms per transaction)
- **Network Impact**: One additional instruction per transaction

## Support

For issues or questions:
1. Check logs for error messages
2. Verify configuration matches deployment
3. Test in devnet before mainnet
4. Contact SolBeast development team

## License

This implementation is closed source. All rights reserved.
