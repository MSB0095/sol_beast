# Bot Operation

## Overview

Sol Beast operates as an automated trading bot that continuously monitors the Solana blockchain for pump.fun token launch events.

## Operation Modes

### Dry Run Mode (Safe)
- Simulates all trading decisions
- No real transactions executed
- Perfect for testing strategies
- **Always start here!**

### Real Mode (Live)
- Executes actual blockchain transactions
- Spends real SOL
- Use with caution
- Requires proper configuration

## Bot Lifecycle

1. **Start** - Begin monitoring blockchain
2. **Monitor** - Watch for token launches
3. **Evaluate** - Apply heuristics to tokens
4. **Execute** - Buy tokens that pass filters
5. **Manage** - Monitor positions for exit conditions
6. **Exit** - Sell when TP/SL/timeout hit

See [Monitoring](./monitoring.md) for details on event detection.
