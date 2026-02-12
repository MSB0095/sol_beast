# Architecture

Technical overview of SOL BEAST's architecture.

## Components

### Backend (Rust)
- **Runtime**: Tokio async
- **WebSocket**: pump.fun event monitoring
- **RPC**: Solana blockchain interaction
- **API**: Axum REST endpoints

### Frontend (React)
- **Framework**: React 18 + TypeScript
- **Build**: Vite 5
- **Styling**: Tailwind CSS 3.3
- **State**: Zustand 4.4

## Data Flow

1. WebSocket receives pump.fun events
2. Event processor evaluates heuristics
3. Buy signal triggers transaction construction
4. Helius Sender submits transaction
5. Holdings monitor tracks positions
6. Dashboard receives real-time updates

See [Introduction](/guide/introduction) for architecture diagram.
