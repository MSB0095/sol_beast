# What is SOL BEAST?

**SOL BEAST** is a high-performance Solana trading bot designed for automated token sniping on pump.fun. Built with Rust for maximum speed and reliability, it monitors blockchain events in real-time, executes trades based on configurable heuristics, and manages positions with advanced risk management features.

## Overview

SOL BEAST combines a powerful Rust backend with a beautiful React dashboard to provide a complete trading automation solution:

- **Monitor** pump.fun token launches in real-time
- **Analyze** tokens based on configurable heuristics
- **Execute** trades with ultra-low latency
- **Manage** positions with automated TP/SL/timeout
- **Track** performance with detailed analytics

## Key Features

### ⚡ Ultra-Low Latency Trading

Integration with [Helius Sender](https://docs.helius.dev) provides:
- Dual routing to validators + Jito infrastructure
- Sub-second transaction confirmation
- Dynamic priority fees and tips
- Regional endpoint selection

### 🎨 Beautiful Dashboard

React/TypeScript frontend featuring:
- 7 cyberpunk-inspired color themes
- Real-time bot status monitoring
- Live holdings and P&L tracking
- Configuration management
- Trade history and analytics

### 🛡️ Risk Management

Protect your capital with:
- Configurable take-profit percentages
- Stop-loss limits
- Timeout-based exits
- Slippage control
- Dry mode for testing

### 🤖 Smart Automation

Heuristic-based trading with:
- Token age filters
- Minimum holder requirements
- Liquidity thresholds
- Manual override capabilities
- Event-driven architecture

## Architecture

```
┌─────────────────────────────────────────────┐
│          React Dashboard (Port 3000)        │
│  ┌───────────┐  ┌──────────┐  ┌─────────┐ │
│  │  Status   │  │ Holdings │  │ Config  │ │
│  └───────────┘  └──────────┘  └─────────┘ │
└────────────────────┬────────────────────────┘
                     │ WebSocket + REST API
┌────────────────────┴────────────────────────┐
│        Rust Backend (Port 8080)             │
│  ┌────────────────────────────────────┐    │
│  │  WebSocket Client (pump.fun events)│    │
│  └──────────────┬─────────────────────┘    │
│                 │                            │
│  ┌──────────────▼──────────────────────┐   │
│  │  Event Processor + Buy Heuristics   │   │
│  └──────────────┬──────────────────────┘   │
│                 │                            │
│  ┌──────────────▼──────────────────────┐   │
│  │  Helius Sender (Transaction Submit) │   │
│  └──────────────┬──────────────────────┘   │
│                 │                            │
│  ┌──────────────▼──────────────────────┐   │
│  │  Holdings Monitor (TP/SL/Timeout)   │   │
│  └─────────────────────────────────────┘   │
└─────────────────────────────────────────────┘
                     │
        ┌────────────┴─────────────┐
        │                          │
┌───────▼────────┐      ┌──────────▼──────┐
│ Solana Mainnet │      │  pump.fun       │
│  Validators    │      │  Program        │
└────────────────┘      └─────────────────┘
```

## How It Works

### 1. Event Monitoring

The bot subscribes to pump.fun events via WebSocket:
```rust
// Monitors for new token creations
// Tracks bonding curve updates
// Detects liquidity changes
```

### 2. Heuristic Evaluation

Each event is evaluated against configured filters:
```toml
min_token_age_seconds = 60
min_holder_count = 50
min_liquidity_sol = 10.0
```

### 3. Trade Execution

When heuristics pass, a buy transaction is constructed:
- Fetch dynamic priority fees from Helius
- Calculate optimal compute units via simulation
- Add tip transfer to Jito account
- Submit via Helius Sender with retry logic

### 4. Position Management

Once a position is acquired, the bot monitors:
- **Take Profit**: Sell when price increases by configured %
- **Stop Loss**: Sell when price decreases by configured %
- **Timeout**: Sell after configured time period

### 5. Performance Tracking

All trades are logged and displayed:
- Entry/exit prices and timestamps
- Profit/loss calculations
- Success/failure reasons
- Performance metrics

## Use Cases

### Token Sniping
- Monitor pump.fun launches
- Auto-buy based on liquidity/holders
- Quick exits at target profits

### Scalping
- Short-term trades with tight TP/SL
- High-frequency with SWQOS mode
- Cost-optimized for volume

### Testing Strategies
- Dry mode for risk-free testing
- Adjustable parameters
- Historical performance review

## Technology Stack

### Backend
- **Language**: Rust 1.70+
- **Runtime**: Tokio (async)
- **HTTP Client**: Reqwest
- **WebSocket**: tungstenite
- **Web Framework**: Axum
- **Blockchain**: Solana SDK 2.1+

### Frontend
- **Framework**: React 18
- **Language**: TypeScript 5.2
- **Build Tool**: Vite 5
- **Styling**: Tailwind CSS 3.3
- **State**: Zustand 4.4
- **Charts**: Recharts 2.10

## Comparison with Alternatives

| Feature | SOL BEAST | Manual Trading | Other Bots |
|---------|-----------|----------------|------------|
| **Speed** | Ultra-fast (Helius) | Slow (human) | Fast |
| **Automation** | Full | None | Partial |
| **Dashboard** | Modern React | N/A | Basic |
| **Themes** | 7 cyberpunk | N/A | Limited |
| **Risk Mgmt** | TP/SL/Timeout | Manual | Limited |
| **Open Source** | Yes | N/A | Rarely |
| **Cost** | $0.001-0.20/tx | Gas only | Variable |

## Next Steps

Ready to get started? Check out:
- [Getting Started Guide](/guide/getting-started) - Installation and first run
- [Configuration Guide](/guide/configuration) - Detailed settings explanation
- [Helius Sender Setup](/guide/helius-sender) - Ultra-low latency configuration

---

::: tip Need Help?
Join our [Discord community](https://discord.gg/ZwyMw3HaDp) or [open an issue](https://github.com/MSB0095/sol_beast/issues) on GitHub.
:::
