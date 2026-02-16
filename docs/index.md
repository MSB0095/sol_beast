---
layout: home

hero:
  name: "SOL BEAST"
  text: "Ultra-Fast Solana Token Sniping Bot"
  tagline: "Automated pump.fun monitoring & trading with real-time dashboard"
  actions:
    - theme: brand
      text: Get Started
      link: /guide/getting-started
    - theme: alt
      text: View on GitHub
      link: https://github.com/MSB0095/sol_beast

features:
  - icon: ⚡
    title: Ultra-Low Latency
    details: Helius Sender integration for sub-second transaction submission with dual routing to validators and Jito infrastructure.

  - icon: 🎨
    title: Beautiful Dashboard
    details: React/TypeScript frontend with 7 cyberpunk themes (Matrix, Neon, Cyber, Plasma, Laser, Gold, Tron).

  - icon: 🛡️
    title: Risk Management
    details: Configurable take-profit, stop-loss, and timeout mechanisms to protect your capital.

  - icon: 🔄
    title: Real-time Monitoring
    details: WebSocket-based live event tracking with automatic reconnection and state management.

  - icon: 🤖
    title: Automated Trading
    details: Heuristic-based buy decisions with manual override capabilities and dry mode for testing.

  - icon: 📊
    title: Performance Analytics
    details: Trade history, P&L tracking, and portfolio insights with detailed performance metrics.

  - icon: 🚀
    title: Rust Backend
    details: Built on Tokio for high-performance concurrent operations with async architecture.

  - icon: 🎯
    title: Smart Filters
    details: Token age, holder count, liquidity checks, and configurable slippage control.

  - icon: 🌍
    title: Regional Endpoints
    details: Choose from 7 global Helius Sender endpoints for optimal latency in your region.
---

<style>
:root {
  --vp-home-hero-name-color: transparent;
  --vp-home-hero-name-background: linear-gradient(90deg, var(--theme-accent) 0%, var(--theme-accent-glow) 100%);
}
</style>

## Quick Start

Get up and running in less than 5 minutes:

```bash
# Clone the repository
git clone https://github.com/MSB0095/sol_beast.git
cd sol_beast

# Configure
cp config.example.toml config.toml
# Edit config.toml with your settings

# Run in dry mode (safe - no real trades)
RUST_LOG=info cargo run

# Launch frontend dashboard
cd frontend && npm install && npm run dev
```

Visit `http://localhost:3000` to access the dashboard.

## Why SOL BEAST?

### 🎯 Built for Speed
Every millisecond counts in token sniping. SOL BEAST uses Helius Sender for ultra-low latency transaction submission, routing through both validators and Jito infrastructure simultaneously.

### 🛡️ Trade Safely
Test strategies risk-free in dry mode. Configure stop-loss and take-profit percentages. Set slippage limits. Monitor actively with real-time dashboard.

### 🎨 Beautiful Interface
7 cyberpunk-inspired themes make monitoring your trades a visual pleasure. Switch between Matrix green, Neon emerald, Cyber blue, and more.

### 🔧 Fully Configurable
Adjust every parameter from trading amounts to heuristic filters. No code changes needed - just edit `config.toml` and restart.

## Features at a Glance

| Feature | Description |
|---------|-------------|
| **Backend** | Rust + Tokio async runtime |
| **Frontend** | React + TypeScript + Vite |
| **Blockchain** | Solana mainnet + pump.fun |
| **Transaction Speed** | Helius Sender (dual routing) |
| **Themes** | 7 cyberpunk color schemes |
| **Risk Management** | TP/SL/timeout automation |
| **Monitoring** | Real-time WebSocket updates |
| **API** | REST endpoints + WebSocket |

## Cost Comparison

| Mode | Tip per Transaction | Cost (at $200/SOL) | Best For |
|------|-------------------|-------------------|----------|
| **Dual Routing** | 0.001 SOL | ~$0.20 | Competitive sniping |
| **SWQOS Only** | 0.000005 SOL | ~$0.001 | High-volume trading |
| **Standard RPC** | 0 SOL | ~$0.00 | Testing/development |

## Community & Support

- 📖 **Documentation**: Comprehensive guides and API reference
- 💬 **Discord**: [Join our community](https://discord.gg/ZwyMw3HaDp) *(Coming Soon)*
- 🐦 **Twitter**: [Follow @Sol__Beast](https://x.com/Sol__Beast)
- 🐛 **Issues**: [Report bugs on GitHub](https://github.com/MSB0095/sol_beast/issues)

## Performance Tips

1. Use **Helius Sender** with dual routing for competitive sniping
2. Choose **regional endpoints** closest to your location
3. Enable **dynamic tips** for auto-adjustment
4. Tune **heuristics** based on your strategy
5. **Monitor actively** especially during initial runs
6. **Start small** and scale up as you gain confidence

## License

SOL BEAST is released under the [MIT License](https://opensource.org/licenses/MIT).

---

<div style="text-align: center; margin-top: 60px; padding: 40px 0; border-top: 2px solid var(--theme-accent);">
  <h2 style="color: var(--theme-accent); text-shadow: 0 0 20px var(--glow-color);">
    ⚡ Built for Speed. Designed for Profit. ⚡
  </h2>
  <p style="color: var(--theme-text-secondary); margin-top: 20px; font-size: 18px;">
    Trade smart. Trade fast. Trade like a beast.
  </p>
</div>
