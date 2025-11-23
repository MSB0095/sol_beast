# Copilot Instructions for sol_beast

## Repository Overview

**sol_beast** is a dual-mode Solana trading bot for monitoring pump.fun token launches with automated buy/sell strategies. It can run:
1. **In the browser** using WebAssembly (WASM) with Solana wallet integration
2. **As a CLI/server** using native Rust for automated trading

## Project Structure

This is a Cargo workspace with three main crates:

```
sol_beast/
├── sol_beast_core/      # Shared Rust library (platform-agnostic)
├── sol_beast_wasm/      # WASM bindings for browser
├── sol_beast_cli/       # CLI application (server mode)
├── frontend/            # React + TypeScript dashboard
└── src/                 # Legacy source (being migrated to workspace structure)
```

### Key Modules in sol_beast_core

- `error.rs` - Common error types using `thiserror`
- `models.rs` - Data models (UserAccount, Holding, TradeRecord, etc.)
- `wallet.rs` - Wallet management with platform-specific storage
- `transaction.rs` - Transaction building for Solana
- `rpc_client.rs` - RPC client abstraction for native/WASM
- `strategy.rs` - Trading strategy logic (TP/SL/timeout)

## Platform-Specific Code

The codebase uses feature flags to separate platform-specific code:

- **`native` feature**: For CLI/server mode (tokio, reqwest, solana-client)
- **`wasm` feature**: For browser mode (wasm-bindgen, web-sys, js-sys)

### Feature Flag Patterns

```rust
// Use async_trait with conditional ?Send bound for WASM compatibility
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait MyTrait {
    async fn my_method(&self) -> Result<()>;
}

// Platform-specific imports
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use tokio::fs;
```

### Storage Patterns

- **WASM**: Uses `localStorage` with `sol_beast_user_` prefix
- **Native**: Uses filesystem in `.sol_beast_data` directory

See `sol_beast_core/src/wallet.rs` for implementation examples.

## Build and Test

### Building the Project

```bash
# Build all crates (native)
cargo build

# Build for release
cargo build --release

# Build specific crate
cargo build -p sol_beast_core
cargo build -p sol_beast_cli

# Build WASM module
cd sol_beast_wasm
wasm-pack build --target web
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Test specific crate
cargo test -p sol_beast_core

# Test with native features
cargo test -p sol_beast_core --features native
```

### Code Quality

```bash
# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets

# Check without building
cargo check
```

### Frontend Development

```bash
cd frontend
npm install
npm run dev      # Development server
npm run build    # Production build
npm run lint     # ESLint
```

## Coding Conventions

### Error Handling

- Use `thiserror` for error types in `sol_beast_core`
- Use `CoreError` for core library errors
- Use `.expect()` with descriptive messages for `Mutex` lock failures in WASM code (they should never fail in single-threaded WASM)

Example from `sol_beast_wasm/src/lib.rs`:
```rust
let mut wallet_manager = WALLET_MANAGER
    .lock()
    .expect("Failed to acquire wallet manager lock");
```

### Async Patterns

- Always use conditional `?Send` bound for async traits to support WASM
- Native code uses `tokio` runtime
- WASM code uses `wasm-bindgen-futures`

### Security

- **Never commit private keys** or secrets
- Browser mode: Private keys stay in wallet extension
- CLI mode: Use environment variables for sensitive data (e.g., `SOL_BEAST_KEYPAIR_B64`)
- Always test with dry-run mode first (`--dry-run` flag)

### Dependencies

- Solana SDK version: 2.1.1
- Use workspace dependencies defined in root `Cargo.toml`
- WASM-compatible versions for browser builds

## Architecture Patterns

### Dual-Mode Design

The core library (`sol_beast_core`) is platform-agnostic and compiled for both:
- Native targets (x86_64, aarch64) with `native` feature
- WASM target (`wasm32-unknown-unknown`) with `wasm` feature

### Transaction Building

Transactions are built using `TransactionBuilder` in `sol_beast_core/src/transaction.rs`:
- Bonding curve PDA calculation
- Token output calculation (constant product formula)
- Slippage tolerance
- Associated Token Account (ATA) creation

### Trading Strategy

Strategy logic in `sol_beast_core/src/strategy.rs` implements:
- Take Profit (TP) percentage
- Stop Loss (SL) percentage  
- Timeout-based auto-sell
- Price filters and safer sniping mode

## Documentation References

- **[README.md](../README.md)** - Main project documentation
- **[ARCHITECTURE.md](../ARCHITECTURE.md)** - Detailed architecture and design
- **[SETUP.md](../SETUP.md)** - Setup instructions for both modes
- **[config.example.toml](../config.example.toml)** - Configuration reference

## Common Tasks

### Adding a New Feature to Core Library

1. Implement in `sol_beast_core/src/` with platform-agnostic code
2. Use feature flags for platform-specific implementations
3. Export from `sol_beast_core/src/lib.rs`
4. Add tests in the same file or `tests/` directory
5. Update WASM bindings in `sol_beast_wasm/src/lib.rs` if needed
6. Update CLI in `sol_beast_cli/src/main.rs` if needed

### Working with WASM

1. Make changes to `sol_beast_wasm/src/lib.rs`
2. Build: `cd sol_beast_wasm && wasm-pack build --target web`
3. Test in frontend: `cd ../frontend && npm run dev`
4. WASM bindings use `#[wasm_bindgen]` attribute for JS interop

### Modifying the Frontend

1. React + TypeScript in `frontend/src/`
2. Wallet integration: `frontend/src/contexts/WalletContextProvider.tsx`
3. WASM store: `frontend/src/store/wasmStore.ts`
4. Components: `frontend/src/components/`

## Important Notes

- The project supports both Browser mode (user controls wallet) and CLI mode (automated trading)
- Always consider platform-specific constraints when modifying core library
- WASM builds are single-threaded and don't support blocking operations
- Native builds can use full tokio async runtime
- Test both modes when making changes to shared code
