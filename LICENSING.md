# sol_beast Licensing and Dev Fee Information

## Overview

**sol_beast** is proprietary software with built-in developer fees and license protection. This ensures ongoing development, support, and prevents unauthorized use.

## Developer Fee (2%)

### What is the Dev Fee?

A 2% developer fee is automatically applied to **every buy and sell transaction**. This fee:

- ✅ Supports ongoing development and maintenance
- ✅ Funds new features and improvements
- ✅ Provides user support and documentation
- ✅ Ensures long-term project sustainability

### How It Works

1. **On Buy Transactions**: 2% of your SOL buy amount is sent to the dev wallet
   - Example: Buying with 1 SOL → 0.02 SOL dev fee
   
2. **On Sell Transactions**: 2% of your SOL proceeds is sent to the dev wallet
   - Example: Selling for 1.5 SOL → 0.03 SOL dev fee

### Configuration

Add your dev fee wallet address to `config.toml`:

```toml
# Dev Fee Configuration (REQUIRED)
dev_fee_wallet = "YOUR_DEV_WALLET_ADDRESS_HERE"
dev_fee_bps = 200  # 2% (DO NOT MODIFY)
```

**⚠️ Important:** 
- The dev fee is **hardcoded at 2%** and cannot be disabled
- Attempting to bypass or modify the dev fee violates the license agreement
- All transactions will include the dev fee transfer

### Why a Dev Fee?

Unlike free open-source projects, sol_beast:
- Provides professional-grade trading algorithms
- Includes advanced features like Helius Sender integration
- Offers dedicated support and documentation
- Receives continuous updates and security patches
- Maintains compatibility with evolving Solana ecosystem

The 2% fee is a fair and transparent way to fund these efforts while keeping the software affordable.

---

## License System

### License Key Requirement

**sol_beast requires a valid license key to operate.** The bot will not start without a properly configured license.

### Getting a License

1. **Contact the Developer**: Reach out to obtain your license key
2. **Receive Your Key**: You'll get a unique license key tied to your deployment
3. **Configure**: Add the license key to your `config.toml`:

```toml
# License Key (REQUIRED)
license_key = "YOUR_LICENSE_KEY_HERE"
```

### License Types

#### 1. Perpetual License
- ✅ No expiration date
- ✅ Lifetime access to current major version
- ✅ Includes all minor updates and patches
- ⚠️ Major version upgrades may require new license

#### 2. Time-Limited License
- ✅ Valid for specified duration (e.g., 1 year)
- ✅ Includes all updates during validity period
- ⚠️ Requires renewal after expiration
- 💡 Automatic expiration warnings at startup

### License Validation

The bot performs license validation at startup:

```
╔═══════════════════════════════════════════════════════════════╗
║                        sol_beast v0.1.0                       ║
║                 Licensed Software - All Rights Reserved       ║
╚═══════════════════════════════════════════════════════════════╝

✓ License key validated successfully
License validated: Standard perpetual license
```

### What the License Protects

The license system ensures:

1. **Anti-Piracy**: Only authorized users can run the bot
2. **Revenue Protection**: Dev fees go to legitimate developers
3. **Support Eligibility**: Licensed users receive official support
4. **Update Access**: Automatic updates only for valid licenses
5. **Code Integrity**: Prevents unauthorized modifications

### Security Features

- 🔒 **Encrypted License Keys**: Keys use cryptographic hashing
- 🔒 **Checksum Verification**: Detects tampered or corrupted keys
- 🔒 **Format Validation**: Ensures key authenticity
- 🔒 **Type Checking**: Validates license type and expiration
- 🔒 **Startup Validation**: Blocks execution without valid license

---

## Code Protection

### Anti-Copying Measures

sol_beast includes multiple layers of protection:

1. **License Validation**: Required at every startup
2. **Dev Fee Integration**: Embedded in transaction logic
3. **Checksum Verification**: Detects code modifications
4. **Proprietary Algorithms**: Not available in public repositories

### Terms of Use

By using sol_beast, you agree to:

- ✅ Use only with a valid license key
- ✅ Pay the 2% dev fee on all transactions
- ✅ Not copy, modify, or redistribute the software
- ✅ Not reverse engineer or decompile the code
- ✅ Not share your license key with others
- ✅ Not attempt to bypass license or fee mechanisms

### Violations and Enforcement

**Unauthorized use may result in:**
- ❌ License revocation
- ❌ Loss of support and updates
- ❌ Legal action for copyright infringement
- ❌ Permanent ban from future licensing

**We detect:**
- Modified code that bypasses dev fees
- Shared or leaked license keys
- Unauthorized distributions
- Commercial resale without authorization

---

## FAQ

### Q: Can I disable the dev fee?
**A:** No. The 2% dev fee is mandatory and built into the core transaction logic. Attempting to remove it violates the license agreement and will result in license revocation.

### Q: Can I use the same license on multiple servers?
**A:** No. Each license is tied to a specific deployment. Contact the developer for multi-server licensing options.

### Q: What happens if my license expires?
**A:** For time-limited licenses, the bot will stop working after expiration. You'll receive warnings 7 days before expiration. Contact the developer to renew.

### Q: Can I modify the code?
**A:** No. sol_beast is proprietary software. Modifications violate the license agreement. If you need custom features, contact the developer.

### Q: Is the source code available?
**A:** The source is provided for transparency and self-hosting, but remains proprietary. You cannot redistribute or create derivative works.

### Q: How do I get support?
**A:** Only licensed users receive official support. Contact the developer with your license key for assistance.

### Q: Can I get a refund on dev fees?
**A:** Dev fees are non-refundable as they're automatically processed on-chain. They support ongoing development that benefits all users.

---

## Compliance

### For Developers Who Want to Fork

If you're inspired by sol_beast and want to create your own trading bot:

✅ **DO:**
- Build your own bot from scratch
- Use public Solana libraries and documentation
- Create your own algorithms and strategies
- Implement your own fee structures

❌ **DON'T:**
- Copy sol_beast's code or architecture
- Reuse proprietary components
- Bypass or remove licensing mechanisms
- Claim your work is "based on" sol_beast without authorization

### Open Source Alternatives

If you prefer open-source software, consider:
- Building your own bot using public Solana SDKs
- Using truly open-source alternatives (if available)
- Contributing to open-source Solana projects

**sol_beast is NOT open source** despite source code visibility for self-hosting purposes.

---

## Contact

**For licensing inquiries, support, or custom development:**

- 📧 Email: [Contact Developer]
- 🔗 Repository Issues: For licensed users only
- 💬 Discord/Telegram: [Community Link]

**Response time:** 24-48 hours for licensed users

---

## Legal

© 2025 sol_beast. All Rights Reserved.

This software is protected by copyright law and international treaties. Unauthorized reproduction or distribution of this software, or any portion of it, may result in severe civil and criminal penalties, and will be prosecuted to the maximum extent possible under law.

**Version:** 1.0  
**Last Updated:** January 2025
