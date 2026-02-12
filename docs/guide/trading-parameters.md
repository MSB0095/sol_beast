# Trading Parameters

Detailed guide on optimizing trading parameters for your strategy.

## Core Parameters

- **Buy Amount**: How much SOL to spend per trade
- **Slippage**: Maximum acceptable price deviation
- **Take Profit**: Exit at X% profit
- **Stop Loss**: Exit at X% loss
- **Timeout**: Exit after X seconds

See the [Configuration Guide](/guide/configuration) for detailed explanations and examples.

## Strategy Templates

### Scalping
Quick trades with tight profit targets:
- TP: 5-15%
- SL: 10-15%
- Timeout: 60-180 seconds

### Swing Trading
Medium-term holds:
- TP: 30-100%
- SL: 20-30%
- Timeout: 300-900 seconds

### HODL
Long-term holds:
- TP: 100-500%
- SL: 40-50%
- Timeout: 900-3600 seconds
