# Helius Sender Integration

Ultra-low latency transaction submission for competitive trading.

## Overview

[Helius Sender](https://docs.helius.dev/solana-rpc-nodes/sending-transactions-on-solana/sender) provides optimized transaction routing for maximum speed and inclusion probability.

## Quick Setup

```toml
helius_sender_enabled = true
helius_min_tip_sol = 0.001
helius_priority_fee_multiplier = 1.2
```

## Routing Modes

### Dual Routing (Fast)
- Routes to validators + Jito simultaneously
- Cost: 0.001 SOL/tx (~$0.20)
- Best for competitive sniping

### SWQOS Only (Cheap)
- Routes via SWQOS infrastructure
- Cost: 0.000005 SOL/tx (~$0.001)
- Best for high-volume trading

## Configuration Details

See [Configuration Guide](/guide/configuration) and the main [README](https://github.com/MSB0095/sol_beast#helius-sender-integration) for complete details.
