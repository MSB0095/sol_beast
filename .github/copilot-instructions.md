# Instructions for Contributing to Sol Beast

## Overview
Sol Beast is a Rust-based asynchronous service designed for monitoring pump.fun events on the Solana blockchain. It automates token purchases under specific heuristics and manages holdings with features like take-profit (TP), stop-loss (SL), and timeout mechanisms. The project also includes a React + TypeScript frontend for real-time monitoring and configuration.

## Key Components

### Backend (Rust)
- **`src/main.rs`**: Entry point for the application. Handles runtime, message processing, and holdings monitoring.
- **`src/ws.rs`**: Manages WebSocket subscriptions and reconnection logic.
- **`src/rpc.rs`**: Contains Solana RPC helpers, price extraction, and buy/sell functions (with TODOs for transaction construction).
- **`src/models.rs`**: Defines bonding curve state and models.
- **`src/helius_sender.rs`**: Integrates with Helius Sender for ultra-low latency transaction submission.

### Frontend (React + TypeScript)
- **`frontend/src/components`**: Contains reusable UI components like `Dashboard`, `ConfigurationPanel`, and `TradingPerformanceWidget`.
- **`frontend/src/services/api.ts`**: Handles API interactions with the backend.
- **`frontend/src/store`**: Manages application state using Zustand.
- **`frontend/src/contexts/WalletContextProvider.tsx`**: Provides wallet context for the application.

## Developer Workflows

### Backend
1. **Setup Configuration**:
   - Copy `config.example.toml` to `config.toml`.
   - Edit `config.toml` to set `wallet_keypair_path` and other required values.

2. **Run in Dry Mode**:
   ```bash
   RUST_LOG=info cargo run
   ```

3. **Run in Real Mode** (after setting up `wallet_keypair_path`):
   ```bash
   RUST_LOG=info cargo run --release -- --real
   ```

4. **Build for Production**:
   ```bash
   cargo build --release
   ```

### Frontend
1. **Install Dependencies**:
   ```bash
   cd frontend
   npm install
   ```

2. **Start Development Server**:
   ```bash
   npm run dev
   ```
   - Access the frontend at `http://localhost:3000`.
   - API requests are proxied to `http://localhost:8080/api`.

3. **Build for Production**:
   ```bash
   npm run build
   ```

## Project-Specific Conventions
- **Helius Sender Integration**:
  - Dual routing and SWQOS-only modes for transaction submission.
  - Dynamic priority fees and tips fetched from Helius and Jito APIs.
  - Configuration options in `config.toml` (e.g., `helius_sender_enabled`, `helius_min_tip_sol`).

- **Logging**:
  - Use `RUST_LOG` for backend logging.
  - Frontend logs are managed via browser developer tools.

- **State Management**:
  - Frontend uses Zustand for global state management.

## External Dependencies
- **Helius Sender**: For low-latency transaction submission.
- **Jito Infrastructure**: For competitive trading scenarios.
- **Solana RPC**: For blockchain interactions.

## Testing and Debugging
- **Backend**:
  - Use `RUST_LOG=debug` for detailed logs.
  - Review `src/rpc.rs` and `src/helius_sender.rs` for transaction-related issues.

- **Frontend**:
  - Use `npm run dev` for hot-reloading during development.
  - Check WebSocket connections for real-time updates.

## Key Files and Directories
- **Backend**:
  - `src/main.rs`, `src/ws.rs`, `src/rpc.rs`, `src/models.rs`, `src/helius_sender.rs`
- **Frontend**:
  - `frontend/src/components`, `frontend/src/services/api.ts`, `frontend/src/store`, `frontend/src/contexts/WalletContextProvider.tsx`

## Notes
- Do not commit private keys or sensitive configuration files.
- Review and implement TODOs in `src/rpc.rs` before enabling real trading.
- Follow the configuration examples in `config.example.toml` for setting up the project.