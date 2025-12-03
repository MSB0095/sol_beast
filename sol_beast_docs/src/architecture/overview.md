# Architecture Overview

Sol Beast is designed with a unique **platform-agnostic core** architecture that enables it to run in two completely different environments with zero code duplication.

## 🎯 Design Philosophy

### Core Principles

1. **Write Once, Deploy Anywhere** - Business logic lives in a shared core library
2. **Zero Duplication** - No copy-paste between WASM and backend implementations  
3. **Platform Abstraction** - Abstract platform-specific functionality behind traits
4. **Type Safety** - Leverage Rust's type system for correctness
5. **Performance** - Native performance in both deployment modes

## 📦 Component Breakdown

### Core Library (`sol_beast_core`)

The heart of Sol Beast - **100% platform-agnostic**.

**Responsibilities:**
- Token analysis and heuristics evaluation
- Trading decision logic
- Transaction construction
- Position management (TP/SL/timeout)
- Settings management
- Risk calculations

### Platform Adapters

**WASM Adapter** (`sol_beast_wasm`)
- Browser WebSocket via fetch API
- localStorage for persistence
- Solana Wallet Adapter integration
- JavaScript/TypeScript bindings

**Native Adapter** (`sol_beast_cli`)
- tokio-tungstenite for WebSocket
- File-based configuration
- Native Solana RPC client
- Keypair-based signing

## 🚀 Benefits

### 1. Code Reuse
- Core trading logic: **100% shared**
- Bug fixes benefit both modes automatically
- New features work everywhere

### 2. Testability
- Core logic has zero platform dependencies
- Unit tests run without mock setup
- Integration tests per platform

### 3. Flexibility
- Deploy to GitHub Pages (free!)
- Or run on your own server
- Same codebase, different deployment

## 📚 Further Reading

- [Core Components](./core-components.md)
- [WASM Mode](./wasm-mode.md)
- [Backend Mode](./backend-mode.md)
- [Project Structure](./project-structure.md)
