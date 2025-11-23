# SolBeast Dev Fee Smart Contract

## Overview

This is an ultra-compact Solana smart contract (<500 bytes) designed to enforce a 2% dev fee on all buy and sell transactions in the SolBeast platform. The contract includes anti-copycat protection through obfuscated magic codes.

## Features

- **Compact Size**: Compiled to under 500 bytes
- **2% Dev Fee**: Automatically collects 2% of transaction amount in SOL
- **Anti-Copycat Protection**: Two obfuscated magic codes embedded in the contract
  - Magic Code 1: XOR with 0x42
  - Magic Code 2: XOR with 0x7F
- **Hardcoded Dev Wallet**: Fee recipient address is hardcoded in the contract
- **Operation Type Support**: Distinguishes between buy (0) and sell (1) operations

## Security Features

### Magic Code Validation

The contract validates two 8-byte magic codes that must be XOR-encoded in the transaction instruction data:

1. **Magic Code 1**: Validated with XOR 0x42
2. **Magic Code 2**: Validated with XOR 0x7F

These magic codes make it extremely difficult for copycats to create a zero-fee version since:
- The codes are obfuscated in the bytecode
- They must be provided correctly in each transaction
- The backend enforces their inclusion
- Reverse engineering is complicated by the XOR encoding

### Dev Wallet Validation

The dev wallet address is hardcoded in the contract as a 32-byte array. Any transaction that doesn't transfer to this exact address will fail.

## Building the Contract

### Prerequisites

Install Solana CLI tools:
```bash
sh -c "$(curl -sSfL https://release.solana.com/stable/install)"
```

### Build Instructions

1. Navigate to the program directory:
```bash
cd program
```

2. Build the contract:
```bash
cargo build-sbf
```

3. Verify the binary size:
```bash
ls -lh target/deploy/solbeast_dev_fee.so
```

The compiled `.so` file should be under 500 bytes.

## Deployment

### Before Deployment

1. Update the `DEV_WALLET` constant in `src/lib.rs` with your actual dev wallet address
2. (Optional) Regenerate magic codes for additional security

### Deploy to Devnet (Testing)

```bash
solana config set --url devnet
solana program deploy target/deploy/solbeast_dev_fee.so
```

### Deploy to Mainnet (Production)

```bash
solana config set --url mainnet-beta
solana program deploy target/deploy/solbeast_dev_fee.so
```

Save the Program ID that is returned after deployment.

## Integration with Backend

After deployment, update the backend configuration:

1. Update `src/dev_fee.rs`:
   - Replace `DEV_FEE_PROGRAM_ID` with your deployed program ID
   - Replace `DEV_WALLET` with your dev wallet address

2. Enable dev fee in `config.toml`:
```toml
dev_fee_enabled = true
dev_wallet_address = "YourDevWalletAddressHere"
```

## How It Works

1. **Transaction Creation**: When a user initiates a buy or sell, the backend adds a dev fee instruction
2. **Magic Code Embedding**: The backend includes the XOR-encoded magic codes in the instruction data
3. **Smart Contract Validation**: The on-chain program validates:
   - Magic codes are correct (XOR decoded)
   - Dev wallet matches hardcoded address
   - Payer has signed the transaction
4. **Fee Transfer**: 2% of the payer's balance is transferred to the dev wallet
5. **Main Operation**: If validation passes, the main buy/sell operation proceeds

## Anti-Copycat Measures

This contract is designed to be difficult to copy or bypass:

1. **Closed Source Bytecode**: While the source is visible here, the compiled bytecode makes reverse engineering difficult
2. **Obfuscated Magic Codes**: XOR encoding with random-looking byte sequences
3. **Backend Enforcement**: The SolBeast backend is the only entity that knows how to generate valid instruction data
4. **Hardcoded Addresses**: Dev wallet is embedded in the bytecode
5. **Bytecode Validation**: The backend can verify the exact bytecode of the deployed program

## Bypassing Protection (Why It's Hard)

To create a copycat without fees, an attacker would need to:

1. Reverse engineer the XOR magic codes from the bytecode
2. Understand the exact instruction data format
3. Modify the contract to remove fee validation
4. Deploy their own version
5. Modify their frontend/backend to use the new contract

The obfuscation and backend integration make this significantly more effort than the 2% fee is worth.

## Testing

Run the included tests:

```bash
cargo test
```

## License

Closed source - All rights reserved

## Support

For deployment assistance or questions, contact the SolBeast team.
