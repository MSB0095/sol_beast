# Dev Fee Deployment Checklist

Use this checklist when deploying the dev fee system to production.

## Pre-Deployment

### 1. Configure Dev Wallet Address

- [ ] Decide on dev wallet address for receiving fees
- [ ] Ensure wallet exists and you have the private key
- [ ] Fund wallet with small amount for testing

### 2. Update Smart Contract

**File**: `program/src/lib.rs`

- [ ] Convert dev wallet address to byte array:
  ```bash
  # Get the pubkey bytes
  solana address --keypair <your-wallet.json>
  # Convert base58 to bytes using a tool or script
  ```

- [ ] Update `DEV_WALLET` constant (line ~22):
  ```rust
  const DEV_WALLET: [u8; 32] = [
      // Replace with actual bytes from your dev wallet
      // Example: [142, 123, 45, ...]
  ];
  ```

- [ ] (Optional) Regenerate magic codes for additional security:
  ```rust
  // Generate new random 8-byte sequences
  const M1: [u8; 8] = [/* new random bytes */];
  const M2: [u8; 8] = [/* new random bytes */];
  ```

### 3. Build Smart Contract

- [ ] Navigate to program directory:
  ```bash
  cd program
  ```

- [ ] Build for production:
  ```bash
  ./build.sh
  ```

- [ ] Verify size is under 500 bytes:
  ```bash
  ls -lh target/deploy/solbeast_dev_fee.so
  ```

- [ ] Run tests:
  ```bash
  cargo test
  ```

## Deployment to Devnet (Testing)

### 4. Deploy Contract to Devnet

- [ ] Configure Solana CLI for devnet:
  ```bash
  solana config set --url devnet
  ```

- [ ] Check your balance (need SOL for deployment):
  ```bash
  solana balance
  ```

- [ ] If needed, airdrop devnet SOL:
  ```bash
  solana airdrop 2
  ```

- [ ] Deploy contract:
  ```bash
  solana program deploy target/deploy/solbeast_dev_fee.so
  ```

- [ ] Save the Program ID returned by deployment

### 5. Update Backend for Devnet

**File**: `src/dev_fee.rs`

- [ ] Update `DEV_FEE_PROGRAM_ID` (line ~18):
  ```rust
  const DEV_FEE_PROGRAM_ID: &str = "YOUR_DEPLOYED_PROGRAM_ID_HERE";
  ```

- [ ] Update `DEV_WALLET` (line ~23):
  ```rust
  const DEV_WALLET: &str = "YOUR_DEV_WALLET_ADDRESS_HERE";
  ```

- [ ] If you changed magic codes, update `M1` and `M2` accordingly

**File**: `config.toml`

- [ ] Enable dev fee:
  ```toml
  dev_fee_enabled = true
  ```

- [ ] (Optional) Override wallet:
  ```toml
  dev_wallet_address = "YourDevWalletAddressHere"
  ```

- [ ] Set devnet RPC URLs:
  ```toml
  solana_rpc_urls = ["https://api.devnet.solana.com"]
  solana_ws_urls = ["wss://api.devnet.solana.com/"]
  ```

### 6. Test on Devnet

- [ ] Build backend:
  ```bash
  cd ..  # Back to root
  cargo build --release
  ```

- [ ] Run in dry mode first:
  ```bash
  RUST_LOG=info cargo run --release
  ```

- [ ] Check logs for "Added 2% dev fee" messages

- [ ] Test small buy transaction:
  ```bash
  RUST_LOG=info cargo run --release -- --real
  ```

- [ ] Verify transaction on Solscan (devnet):
  - Check dev fee transfer appears
  - Verify correct amount (2%)
  - Confirm transfer to dev wallet

- [ ] Check dev wallet balance increased:
  ```bash
  solana balance <DEV_WALLET_ADDRESS>
  ```

- [ ] Test sell transaction (after holding period)

- [ ] Monitor logs for any errors

## Deployment to Mainnet (Production)

### 7. Final Pre-Flight Checks

- [ ] All devnet tests passed successfully
- [ ] Dev wallet address is correct
- [ ] Magic codes match between contract and backend
- [ ] Build artifacts are from latest source
- [ ] No debug or test code in production builds

### 8. Deploy Contract to Mainnet

- [ ] Configure Solana CLI for mainnet:
  ```bash
  solana config set --url mainnet-beta
  ```

- [ ] Check your balance (need ~5 SOL for deployment + buffer):
  ```bash
  solana balance
  ```

- [ ] **IMPORTANT**: Double-check contract code one more time:
  ```bash
  cat program/src/lib.rs | grep -A 5 "DEV_WALLET"
  ```

- [ ] Deploy contract:
  ```bash
  solana program deploy target/deploy/solbeast_dev_fee.so
  ```

- [ ] Save the mainnet Program ID

- [ ] Verify deployment on Solana Explorer:
  ```
  https://explorer.solana.com/address/<PROGRAM_ID>
  ```

### 9. Update Backend for Mainnet

**File**: `src/dev_fee.rs`

- [ ] Update `DEV_FEE_PROGRAM_ID` with mainnet program ID

- [ ] Verify `DEV_WALLET` matches deployed contract

**File**: `config.toml`

- [ ] Update RPC URLs to mainnet:
  ```toml
  solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]
  solana_ws_urls = ["wss://api.mainnet-beta.solana.com/"]
  ```

- [ ] (Recommended) Use paid RPC service for production

- [ ] Enable dev fee:
  ```toml
  dev_fee_enabled = true
  ```

### 10. Production Deployment

- [ ] Build production backend:
  ```bash
  cargo build --release
  ```

- [ ] Upload binary to production server

- [ ] Update production config.toml

- [ ] Set wallet keypair securely

- [ ] Start service:
  ```bash
  ./start.sh
  ```

### 11. Post-Deployment Monitoring

- [ ] Monitor first few transactions closely

- [ ] Check logs for dev fee messages:
  ```bash
  tail -f logs/sol_beast.log | grep "dev fee"
  ```

- [ ] Verify dev wallet receiving fees:
  ```bash
  solana balance <DEV_WALLET_ADDRESS>
  ```

- [ ] Check transaction explorer for fee transfers

- [ ] Set up alerts for:
  - Failed transactions
  - Missing dev fees
  - Wallet balance threshold

### 12. Backup and Documentation

- [ ] Save program ID in secure location

- [ ] Document deployment date and version

- [ ] Keep copy of deployed `.so` file

- [ ] Store wallet backup securely

- [ ] Document any custom magic codes used

## Rollback Plan

If something goes wrong:

1. **Disable Dev Fee Immediately**:
   ```toml
   dev_fee_enabled = false
   ```
   Restart backend

2. **Investigate Issue**:
   - Check logs for errors
   - Verify contract deployment
   - Test transaction manually

3. **Fix and Redeploy**:
   - Fix issue in code
   - Test on devnet
   - Redeploy to mainnet if needed

## Security Notes

- [ ] Never commit private keys to git
- [ ] Keep magic codes secret (not in public repos)
- [ ] Rotate magic codes periodically
- [ ] Monitor for unusual transaction patterns
- [ ] Set up automated balance alerts
- [ ] Test disaster recovery procedures

## Support Contacts

- Dev Team: [Your contact info]
- Solana Support: https://discord.gg/solana
- Emergency Hotline: [Your emergency contact]

## Sign-Off

- [ ] Smart contract deployed and verified: ____________
- [ ] Backend deployed and tested: ____________
- [ ] Monitoring set up: ____________
- [ ] Documentation complete: ____________

Deployment completed by: _________________ Date: _________

Notes:
_____________________________________________________________
_____________________________________________________________
_____________________________________________________________
