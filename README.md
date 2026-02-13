<div align="center">

# 🦁 SOL BEAST

### Ultra-Fast Solana Token Sniping Bot

*Automated pump.fun monitoring & trading with real-time dashboard*

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/react-%2320232a.svg?style=for-the-badge&logo=react&logoColor=%2361DAFB)](https://reactjs.org/)
[![Solana](https://img.shields.io/badge/Solana-000?style=for-the-badge&logo=solana&logoColor=9945FF)](https://solana.com/)
[![TypeScript](https://img.shields.io/badge/typescript-%23007ACC.svg?style=for-the-badge&logo=typescript&logoColor=white)](https://www.typescriptlang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

[Features](#-features) • [Quick Start](#-quick-start) • [Configuration](#-configuration) • [Documentation](docs/README.md) • [Contributing](#-contributing)

</div>

---

## 📖 About

**SOL BEAST** is a high-performance Solana trading bot designed for automated token sniping on pump.fun. Built with Rust for maximum speed and reliability, it monitors blockchain events in real-time, executes trades based on configurable heuristics, and manages positions with advanced risk management features.

### 🎯 Key Highlights

- **⚡ Ultra-Low Latency**: Helius Sender integration for sub-second transaction submission
- **🎨 Beautiful Dashboard**: React/TypeScript frontend with 7 cyberpunk themes
- **🛡️ Risk Management**: Configurable take-profit, stop-loss, and timeout mechanisms
- **🔄 Real-time Monitoring**: WebSocket-based live event tracking
- **🤖 Automated Trading**: Heuristic-based buy decisions with manual override
- **📊 Performance Analytics**: Trade history, P&L tracking, and portfolio insights

---

## ✨ Features

### Backend (Rust)
- **Async Architecture**: Built on Tokio for high-performance concurrent operations
- **WebSocket Subscriptions**: Real-time pump.fun event monitoring with auto-reconnect
- **Helius Sender Integration**: Dual routing (validators + Jito) for maximum speed
- **Dynamic Priority Fees**: Auto-fetched from Helius API with configurable multipliers
- **Smart Contract Interaction**: Full Solana program integration
- **Position Management**: Automatic TP/SL/timeout execution
- **REST API**: Axum-based endpoints for frontend communication

### Frontend (React + TypeScript)
- **Live Dashboard**: Real-time bot status, holdings, and trade history
- **7 Cyberpunk Themes**: Matrix, Neon, Cyber, Plasma, Laser, Gold, Tron
- **Configuration Panel**: Adjust settings without restarting
- **Performance Widgets**: Trade metrics, P&L charts, success rates
- **Wallet Integration**: Solana wallet connect support
- **Responsive Design**: Works on desktop and mobile

### Trading Features
- **Heuristic Filters**: Token age, holder count, liquidity checks
- **Slippage Control**: Configurable max slippage per trade
- **Position Sizing**: Dynamic buy amounts based on settings
- **Multiple Exit Strategies**: TP%, SL%, timeout-based exits
- **Dry Mode**: Test strategies without risking capital

---

## 🚀 Quick Start

### Prerequisites

- **Rust** 1.70+ ([Install](https://rustup.rs/))
- **Node.js** 18+ ([Install](https://nodejs.org/))
- **Solana CLI** ([Install](https://docs.solana.com/cli/install-solana-cli-tools))
- **Solana Wallet** with SOL for trading

### Installation

1. **Clone the repository**
```bash
git clone https://github.com/MSB0095/sol_beast.git
cd sol_beast
```

2. **Configure the bot**
```bash
cp config.example.toml config.toml
```

Edit `config.toml` and set your values:
```toml
# Required: Set your wallet keypair path
wallet_keypair_path = "/path/to/your/keypair.json"

# Required: Set your Solana RPC endpoint
solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]

# Optional: Enable Helius Sender for ultra-low latency
helius_sender_enabled = true
helius_min_tip_sol = 0.001
```

3. **Install frontend dependencies**
```bash
cd frontend
npm install
cd ..
```

### Running the Bot

**Dry Mode** (Safe - No Real Trades):
```bash
RUST_LOG=info cargo run
```

**Real Mode** (Live Trading):
```bash
RUST_LOG=info cargo run --release -- --real
```

**Launch Frontend** (Separate Terminal):
```bash
cd frontend
npm run dev
```

Access the dashboard at: `http://localhost:3000`

---

## ⚙️ Configuration

### Basic Settings

```toml
# Trading Parameters
buy_amount_sol = 0.05              # Amount per trade in SOL
max_slippage_bps = 500             # 5% max slippage
take_profit_percentage = 50.0      # Exit at +50% profit
stop_loss_percentage = 20.0        # Exit at -20% loss
timeout_seconds = 300              # Exit after 5 minutes

# Heuristic Filters
min_token_age_seconds = 60         # Skip tokens newer than 60s
min_holder_count = 50              # Require at least 50 holders
min_liquidity_sol = 10.0           # Require 10 SOL liquidity
```

### Advanced: Helius Sender

For competitive trading, enable Helius Sender for ultra-low latency:

```toml
helius_sender_enabled = true
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
helius_min_tip_sol = 0.001
helius_priority_fee_multiplier = 1.2
helius_use_dynamic_tips = true     # Auto-adjust based on network
```

**Cost Comparison:**
- **Dual Routing** (validators + Jito): 0.001 SOL/tx (~$0.20)
- **SWQOS Only**: 0.000005 SOL/tx (~$0.001) - 200x cheaper!

See [Helius Integration Guide](#helius-sender-integration) for details.

---

## 📁 Project Structure

```
sol_beast/
├── src/                      # Rust backend
│   ├── main.rs              # Entry point & runtime
│   ├── ws.rs                # WebSocket subscriptions
│   ├── rpc.rs               # Solana RPC & trading
│   ├── helius_sender.rs     # Ultra-low latency tx submission
│   ├── buyer.rs             # Buy logic & heuristics
│   ├── monitor.rs           # Position monitoring
│   ├── api.rs               # REST API endpoints
│   └── models.rs            # Data structures
│
├── frontend/                 # React dashboard
│   ├── src/
│   │   ├── components/      # UI components
│   │   ├── services/        # API & WebSocket
│   │   ├── store/           # State management
│   │   └── index.css        # Theme definitions
│   ├── package.json
│   └── vite.config.ts
│
├── docs/                     # Documentation website
├── config.example.toml       # Configuration template
├── Cargo.toml               # Rust dependencies
└── README.md                # You are here
```

---

## 🔧 Helius Sender Integration

SOL BEAST supports optional ultra-low latency transaction submission via [Helius Sender](https://docs.helius.dev/solana-rpc-nodes/sending-transactions-on-solana/sender). When enabled, transactions are routed through optimized infrastructure for maximum speed.

### Features

- ⚡ **Dual Routing**: Send to validators + Jito simultaneously
- 🎯 **Dynamic Priority Fees**: Auto-fetched from Helius API
- 💰 **Smart Tips**: Configurable minimum with dynamic adjustment
- 🔄 **Auto Retry**: Built-in retry with exponential backoff
- 🌍 **Regional Endpoints**: Choose closest endpoint for latency

### Quick Setup

```toml
helius_sender_enabled = true
helius_min_tip_sol = 0.001
helius_priority_fee_multiplier = 1.2
```

### Routing Modes

**1. Dual Routing (Recommended for Speed)**
```toml
helius_use_swqos_only = false
helius_min_tip_sol = 0.001  # Min 0.001 SOL required
```
- Routes to **both** validators AND Jito
- Lowest latency, highest inclusion rate
- Best for competitive sniping
- Cost: ~$0.20/tx at $200/SOL

**2. SWQOS-Only (Cost Optimized)**
```toml
helius_use_swqos_only = true
helius_min_tip_sol = 0.000005  # Min 0.000005 SOL
```
- Routes via SWQOS infrastructure only
- Good performance, much lower cost
- Best for high-volume trading
- Cost: ~$0.001/tx - **200x cheaper!**

### Dynamic Tips

Enable auto-adjustment based on network conditions:
```toml
helius_use_dynamic_tips = true
```
- Fetches 75th percentile from Jito API
- Stays competitive during high-traffic launches
- Saves SOL during quiet periods
- Falls back to minimum if API unavailable

### Regional Endpoints

For backend/server applications, use regional HTTP endpoints:
```toml
helius_sender_endpoint = "http://slc-sender.helius-rpc.com/fast"  # Salt Lake City
# Also available: ewr (Newark), lon (London), fra (Frankfurt), 
# ams (Amsterdam), sg (Singapore), tyo (Tokyo)
```

Frontend applications should use global HTTPS:
```toml
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
```

### Cost Breakdown

| Component | Cost (per tx) | Notes |
|-----------|---------------|-------|
| Helius Sender API | Free | No credit consumption |
| Tip (Dual) | 0.001 SOL | ~$0.20 |
| Tip (SWQOS) | 0.000005 SOL | ~$0.001 |
| Priority Fee | 0.00001-0.0001 SOL | Based on congestion |
| **Total (Dual)** | **~0.001 SOL** | ~$0.20 |
| **Total (SWQOS)** | **~0.000005 SOL** | ~$0.001 |

### Resources

- [Helius Sender Docs](https://docs.helius.dev/solana-rpc-nodes/sending-transactions-on-solana/sender)
- [Jito Tips Best Practices](https://docs.jito.wtf/lowlatencytxnsend/#tips)
- [Priority Fee API](https://docs.helius.dev/solana-rpc-nodes/priority-fee-api)

---

## 🎨 Frontend Themes

The dashboard includes 7 cyberpunk-inspired color themes:

| Theme | Name | Color | Best For |
|-------|------|-------|----------|
| 🟢 | MATRIX | `#00ff41` | Classic hacker aesthetic |
| 💎 | NEON | `#10ffb0` | Vibrant emerald glow |
| 🔵 | CYBER | `#00d9ff` | Cool cyan vibes |
| 🟣 | PLASMA | `#d946ef` | Electric purple |
| 💗 | LASER | `#ff0062` | Hot pink energy |
| 🟡 | GOLD | `#ffb000` | Warm amber tones |
| 🔷 | TRON | `#00ffff` | Pure cyan classic |

Switch themes via the top-right button. Preference is saved automatically.

---

## 🛡️ Safety & Security

### ⚠️ Important Warnings

- **Never commit private keys** to version control
- **Test in dry mode** before risking capital
- **Start with small amounts** when going live
- **Monitor actively** during initial runs
- **Review transaction code** in `src/rpc.rs` before production use

### Dry Mode

Always test strategies in dry mode first:
```bash
RUST_LOG=info cargo run  # No --real flag
```

Dry mode:
- ✅ Monitors events and evaluates heuristics
- ✅ Logs what trades would be made
- ❌ Does NOT send transactions
- ❌ Does NOT require wallet setup

### Production Checklist

Before going live:
- [ ] Wallet keypair secured with proper permissions
- [ ] Configuration values reviewed and tested
- [ ] RPC endpoints are reliable and fast
- [ ] Helius Sender settings optimized for your use case
- [ ] Stop-loss and take-profit percentages are sensible
- [ ] Slippage limits are appropriate
- [ ] Test wallet has sufficient SOL for trades + fees
- [ ] Monitoring/alerting is in place

---

## 📊 API Endpoints

The backend exposes REST API endpoints for frontend communication:

### Configuration
- `GET /api/config` - Get current bot configuration
- `POST /api/config` - Update configuration (requires restart)

### Bot Control
- `GET /api/bot/status` - Get bot status (running/stopped)
- `POST /api/bot/start` - Start the bot
- `POST /api/bot/stop` - Stop the bot

### Holdings
- `GET /api/holdings` - Get all current holdings
- `GET /api/holdings/:mint` - Get specific holding

### Trading
- `GET /api/trades` - Get trade history
- `POST /api/trade/buy` - Manual buy (requires --real)
- `POST /api/trade/sell` - Manual sell (requires --real)

### Health
- `GET /api/health` - Health check

Default API port: `8080`

---

## 🔍 Troubleshooting

### Bot won't start

**Issue**: `Error loading config.toml`
- **Solution**: Copy `config.example.toml` to `config.toml` and edit values

**Issue**: `Wallet keypair not found`
- **Solution**: Set `wallet_keypair_path` to a valid Solana keypair JSON file

### Transactions failing

**Issue**: `Blockhash expired`
- **Solution**: Increase `helius_confirm_timeout_secs` or check RPC latency

**Issue**: `Insufficient SOL`
- **Solution**: Ensure wallet has enough SOL for trades + fees + tips

### Frontend not connecting

**Issue**: Dashboard shows "Offline"
- **Solution**: Ensure backend is running on port 8080
- **Solution**: Check CORS settings in `src/api.rs`

### Performance issues

**Issue**: Slow transaction submission
- **Solution**: Enable Helius Sender with dual routing
- **Solution**: Use regional endpoint closest to your location
- **Solution**: Increase `helius_priority_fee_multiplier`

---

## 🧪 Development

### Building

```bash
# Backend
cargo build --release

# Frontend
cd frontend
npm run build
```

### Testing

```bash
# Run backend tests
cargo test

# Run frontend tests
cd frontend
npm run lint
```

### Code Structure

- **`src/main.rs`**: Application entry point, runtime setup
- **`src/ws.rs`**: WebSocket client for pump.fun events
- **`src/rpc.rs`**: Solana RPC interactions, buy/sell logic
- **`src/helius_sender.rs`**: Helius Sender transaction submission
- **`src/buyer.rs`**: Buy decision heuristics
- **`src/monitor.rs`**: Holdings monitoring, TP/SL/timeout
- **`src/api.rs`**: Axum REST API server
- **`src/models.rs`**: Data structures and types

---

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Guidelines

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Setup

1. Install Rust toolchain
2. Install Node.js and npm
3. Install Solana CLI tools
4. Clone and configure as per Quick Start
5. Run in dry mode for testing

---

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 🔗 Links

- **Documentation**: [Full Docs](docs/README.md)
- **GitHub**: [https://github.com/MSB0095/sol_beast](https://github.com/MSB0095/sol_beast)
- **Issues**: [Report a bug](https://github.com/MSB0095/sol_beast/issues)
- **Discord**: [Join Community](https://discord.gg/solbeast) *(Coming Soon)*
- **Twitter**: [@Sol__Beast](https://x.com/Sol__Beast)

---

## ⚡ Performance Tips

1. **Use Helius Sender** with dual routing for competitive sniping
2. **Choose regional endpoints** closest to your location
3. **Enable dynamic tips** for auto-adjustment
4. **Tune heuristics** based on your strategy
5. **Monitor actively** especially during initial runs
6. **Start small** and scale up as you gain confidence

---

## 🙏 Acknowledgments

- [Helius](https://helius.dev/) - Ultra-low latency infrastructure
- [Jito](https://jito.wtf/) - MEV infrastructure
- [Solana](https://solana.com/) - High-performance blockchain
- [pump.fun](https://pump.fun/) - Token launch platform

---

<div align="center">

**Built with ⚡ by the SOL BEAST team**

*Trade smart. Trade fast. Trade like a beast.*

</div>