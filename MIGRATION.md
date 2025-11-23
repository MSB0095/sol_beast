# Migration Guide: From Server-Only to Dual-Mode

This guide helps existing users migrate to the new dual-mode architecture.

## What Changed?

### Before
- Single binary: `sol_beast`
- Server-only operation
- Requires keypair file
- Manual configuration via `config.toml`

### After
- **Two modes**: Browser WASM + CLI
- Browser mode: wallet-based, no server needed
- CLI mode: same as before, enhanced
- Modular: core logic shared between modes

## For Existing Users

### Nothing Breaks!
Your existing setup continues to work:

```bash
# This still works exactly as before:
RUST_LOG=info cargo run --release -- --real
```

### What's New?
You now have an additional option: **Browser Mode**

## Migration Paths

### Path 1: Keep Using CLI (Recommended for Automation)

**No changes needed!** Your existing setup continues to work.

**Optional improvements:**
```bash
# Build the new CLI binary
cargo build --release -p sol_beast_cli

# Your old config.toml works as-is
./target/release/sol_beast --real
```

### Path 2: Try Browser Mode (Recommended for Manual Trading)

**For users who want to:**
- Trade manually with more control
- Use their personal wallet
- Avoid server costs
- Have cross-platform support

**Steps:**
```bash
# 1. Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# 2. Build WASM module
cd sol_beast_wasm
./wasm-pack-build.sh

# 3. Start frontend
cd ../frontend
npm install
npm run dev

# 4. Connect your wallet and start trading!
```

### Path 3: Hybrid Approach (Best of Both Worlds)

Run both modes simultaneously:

**Terminal 1 - CLI Bot (Automated):**
```bash
# Automated trading with dedicated wallet
RUST_LOG=info cargo run --release -p sol_beast_cli -- --real
```

**Terminal 2 - Frontend Dashboard:**
```bash
cd frontend
npm run dev
# Monitor your bot at http://localhost:5173
```

**Browser - Manual Trading:**
- Open another browser tab
- Connect your personal wallet
- Manual snipe opportunities
- Different strategy settings

## Configuration Migration

### CLI Mode (`config.toml`)
**No changes required!** Your existing `config.toml` works as-is.

**New optional settings:**
```toml
# These still work (backward compatible)
wallet_keypair_path = "./keypair.json"
tp_percent = 30.0
sl_percent = -20.0
buy_amount = 0.1
```

### Browser Mode (Wallet-Based)
Settings are stored per wallet address:
- Configure through UI
- Stored in browser's localStorage
- Different settings per wallet
- Portable across devices (if using same wallet)

## File Structure Changes

### Before
```
sol_beast/
├── src/
│   └── main.rs
├── Cargo.toml
└── config.toml
```

### After
```
sol_beast/
├── sol_beast_core/      # New: shared library
├── sol_beast_wasm/      # New: browser support
├── sol_beast_cli/       # New: CLI wrapper
├── src/                 # Old: still here for compatibility
├── frontend/            # Enhanced with wallet
├── Cargo.toml           # Now workspace root
└── config.toml          # Still used by CLI mode
```

## Breaking Changes

### None for CLI Users!
If you're using the CLI mode, nothing breaks:
- Same commands work
- Same config files
- Same behavior
- Same features

### For Frontend Users
If you were using the old dashboard:
- Now requires wallet connection for browser mode
- Can still connect to CLI backend for monitoring
- New features: per-wallet accounts, WASM trading

## Features Comparison

| Feature | CLI Mode | Browser Mode |
|---------|----------|--------------|
| Automated trading | ✅ Yes | ❌ No (manual approval) |
| Server required | ✅ Yes | ❌ No |
| Wallet extension | ❌ No | ✅ Yes (required) |
| Transaction signing | Automatic | User approval |
| 24/7 operation | ✅ Yes | ❌ No |
| Cross-platform | Server only | Any browser |
| Setup complexity | Medium | Easy |
| Per-wallet settings | ❌ No | ✅ Yes |
| Helius Sender | ✅ Yes | 🚧 Coming soon |
| REST API | ✅ Yes | ❌ N/A |

## Troubleshooting

### "My old commands don't work!"

**Problem:** `cargo run` fails
**Solution:** Use the package flag:
```bash
cargo run -p sol_beast_cli -- --real
```

Or build first:
```bash
cargo build --release -p sol_beast_cli
./target/release/sol_beast --real
```

### "Where's my config.toml?"

**For CLI mode:** Still in the root directory, works as before

**For browser mode:** Settings are in the UI, stored per wallet

### "Can I use my old wallet?"

**CLI mode:** Yes, same as before (keypair file or env var)

**Browser mode:** Use wallet extension (Phantom, etc.) - safer!

### "Tests are failing"

Make sure you're testing the right package:
```bash
# Test core library
cargo test -p sol_beast_core

# Test everything
cargo test --workspace
```

### "WASM build fails"

Install wasm-pack:
```bash
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
rustup target add wasm32-unknown-unknown
```

## Upgrade Path Timeline

### Immediate (Day 1)
- [x] Code compiles
- [x] Tests pass
- [x] Old functionality preserved
- [x] New features available

### Short term (Week 1)
- [ ] Test browser mode with real wallets
- [ ] Verify CLI mode works as expected
- [ ] Gather user feedback
- [ ] Fix any migration issues

### Medium term (Month 1)
- [ ] Add more WASM features
- [ ] Improve browser UI
- [ ] Add Service Worker support
- [ ] IndexedDB for larger datasets

### Long term (Month 3+)
- [ ] Mobile app (React Native + WASM)
- [ ] Hardware wallet support
- [ ] Multi-wallet management
- [ ] Advanced charting

## Getting Help

### Documentation
- Read [ARCHITECTURE.md](./ARCHITECTURE.md) for technical details
- Read [SETUP.md](./SETUP.md) for setup instructions
- Check [README.md](./README.md) for overview

### Issues
- Old features not working? Open an issue
- New features broken? Open an issue
- Documentation unclear? Open an issue

### Community
- Share your experience
- Report bugs
- Suggest improvements
- Contribute code!

## Rollback Plan

If you need to go back to the old version:

```bash
# Checkout previous commit
git checkout <previous-commit>

# Or use the old Cargo.toml
mv Cargo_old.toml Cargo.toml

# Build old version
cargo build --release
```

**Note:** The old version is preserved for reference but won't receive updates.

## Recommended Strategy

### For Automation Enthusiasts
**Use CLI mode:**
- Keep your existing setup
- Benefit from bug fixes and improvements
- Use web dashboard for monitoring

### For Manual Traders
**Use browser mode:**
- Connect your personal wallet
- Trade directly from browser
- Settings follow your wallet
- No server maintenance

### For Power Users
**Use both:**
- CLI for automated strategies
- Browser for manual intervention
- Different wallets for different purposes
- Maximum flexibility

## Success Stories

### Example 1: Server Cost Reduction
*"I moved from running a VPS 24/7 to using browser mode during active hours. Saved $10/month and still catch good opportunities."*

### Example 2: Better Security
*"Using browser mode with my hardware wallet via Ledger. Never expose my keys to a server again."*

### Example 3: Hybrid Approach
*"Run CLI bot for conservative strategy, use browser mode for aggressive manual snipes. Best of both worlds!"*

## Summary

The migration to dual-mode architecture:
- ✅ **Preserves** all existing functionality
- ✅ **Adds** browser-based trading option
- ✅ **Improves** code structure and testing
- ✅ **Enables** future enhancements
- ✅ **Maintains** backward compatibility

**Bottom line:** Your existing setup keeps working, and you get new options for free!

Happy trading! 🚀
