# Dev Tip Implementation

## Overview

This document describes the implementation of the optional dev tip feature for the SolBeast trading bot. The feature allows users to configure tips that are sent to the developer wallet on every buy and sell transaction.

## Features

The dev tip system supports two configurable tip modes that can work independently or together:

1. **Percentage Tip** (`dev_tip_percent`): A percentage of the transaction amount
   - Default: 2.0 (2%)
   - Range: 0.0 or higher
   - Applied as: `(transaction_amount * dev_tip_percent / 100)`

2. **Fixed Tip** (`dev_tip_fixed_sol`): A fixed SOL amount per transaction
   - Default: 0.0 SOL
   - Range: 0.0 or higher
   - Applied as: Fixed amount in SOL

Both tip types apply to **both buy AND sell transactions**.

## Configuration

Users can configure dev tips through:

### 1. CLI Configuration (`config.toml`)
```toml
# Dev tip percentage (default: 2.0 = 2% of transaction amount)
dev_tip_percent = 2.0

# Dev tip fixed amount in SOL per transaction (default: 0.0)
dev_tip_fixed_sol = 0.0
```

### 2. WASM/Frontend Configuration
The frontend provides UI controls in the Configuration Panel under "Dev Tip Configuration":
- Dev Tip % (percentage tip)
- Dev Tip Fixed (SOL) (fixed tip amount)

Settings are persisted in:
- Browser localStorage (WASM mode)
- `bot-settings.json` (static defaults)

## Implementation Details

### Settings Structures

Dev tip fields were added to all Settings structures:

1. **`sol_beast_core/src/settings.rs`** - Core settings used by shared logic
2. **`sol_beast_cli/src/settings.rs`** - CLI-specific settings
3. **`sol_beast_wasm/src/lib.rs`** - WASM-specific BotSettings

Each includes:
```rust
#[serde(default = "default_dev_tip_percent")]
pub dev_tip_percent: f64,
#[serde(default = "default_dev_tip_fixed_sol")]
pub dev_tip_fixed_sol: f64,
```

### Tip Calculation

The calculation function in `sol_beast_cli/src/dev_fee.rs`:

```rust
pub fn calculate_dev_tip(
    amount_lamports: u64, 
    tip_percent: f64, 
    tip_fixed_sol: f64
) -> u64 {
    let percentage_tip = (amount_lamports as f64 * tip_percent / 100.0) as u64;
    let fixed_tip = (tip_fixed_sol * 1_000_000_000.0) as u64;
    percentage_tip + fixed_tip
}
```

### Transaction Integration

Dev tips are added to transactions in two places:

#### Buy Transactions (`sol_beast_cli/src/buyer.rs`)
```rust
if settings.dev_fee_enabled || settings.dev_tip_percent > 0.0 || settings.dev_tip_fixed_sol > 0.0 {
    let transaction_lamports = (sol_amount * 1_000_000_000.0) as u64;
    crate::dev_fee::add_dev_tip_to_instructions(
        &mut all_instrs, 
        &payer.pubkey(), 
        transaction_lamports, 
        settings.dev_tip_percent,
        settings.dev_tip_fixed_sol,
        0
    )?;
}
```

#### Sell Transactions (`sol_beast_cli/src/rpc.rs`)
```rust
if settings.dev_fee_enabled || settings.dev_tip_percent > 0.0 || settings.dev_tip_fixed_sol > 0.0 {
    let sol_received_lamports = (sol_received * 1_000_000_000.0) as u64;
    crate::dev_fee::add_dev_tip_to_instructions(
        &mut all_instrs, 
        &user_pubkey, 
        sol_received_lamports,
        settings.dev_tip_percent,
        settings.dev_tip_fixed_sol,
        1
    )?;
}
```

## Examples

### Example 1: Percentage Only
```toml
dev_tip_percent = 2.0
dev_tip_fixed_sol = 0.0
```
- Buy 1 SOL: Tip = 0.02 SOL (2%)
- Sell 0.5 SOL: Tip = 0.01 SOL (2%)

### Example 2: Fixed Only
```toml
dev_tip_percent = 0.0
dev_tip_fixed_sol = 0.001
```
- Buy 1 SOL: Tip = 0.001 SOL
- Sell 0.5 SOL: Tip = 0.001 SOL

### Example 3: Combined
```toml
dev_tip_percent = 2.0
dev_tip_fixed_sol = 0.001
```
- Buy 1 SOL: Tip = 0.021 SOL (0.02 + 0.001)
- Sell 0.5 SOL: Tip = 0.011 SOL (0.01 + 0.001)

### Example 4: Disabled
```toml
dev_tip_percent = 0.0
dev_tip_fixed_sol = 0.0
```
- No tip instructions added to transactions

## Validation

Settings validation ensures:
- `dev_tip_percent >= 0.0`
- `dev_tip_fixed_sol >= 0.0`

Validation occurs in:
- `sol_beast_core/src/settings.rs::validate()`
- `sol_beast_cli/src/settings.rs::validate()`

## Testing

Unit tests in `sol_beast_cli/src/dev_fee.rs`:

```rust
#[test]
fn test_calculate_dev_tip() {
    // Test percentage only (2% of 1 SOL)
    assert_eq!(calculate_dev_tip(1_000_000_000, 2.0, 0.0), 20_000_000);
    
    // Test fixed only (0.01 SOL)
    assert_eq!(calculate_dev_tip(1_000_000_000, 0.0, 0.01), 10_000_000);
    
    // Test both (2% + 0.01 SOL fixed)
    assert_eq!(calculate_dev_tip(1_000_000_000, 2.0, 0.01), 30_000_000);
    
    // Test zero tip
    assert_eq!(calculate_dev_tip(1_000_000_000, 0.0, 0.0), 0);
}
```

## Files Modified

### Core Settings
- `sol_beast_core/src/settings.rs` - Added fields, validation, merge logic

### CLI
- `sol_beast_cli/src/settings.rs` - Added fields, validation, merge logic
- `sol_beast_cli/src/dev_fee.rs` - Added tip calculation and instruction builder
- `sol_beast_cli/src/buyer.rs` - Integrated dev tips in buy transactions
- `sol_beast_cli/src/rpc.rs` - Integrated dev tips in sell transactions
- `config.example.toml` - Added configuration documentation

### WASM
- `sol_beast_wasm/src/lib.rs` - Added fields to BotSettings

### Frontend
- `frontend/src/store/settingsStore.ts` - Added TypeScript interface fields
- `frontend/src/components/ConfigurationPanel.tsx` - Added UI controls
- `frontend/public/bot-settings.json` - Added default values

## Migration Notes

Existing configurations without these fields will use the defaults:
- `dev_tip_percent`: 2.0 (2%)
- `dev_tip_fixed_sol`: 0.0 SOL

This means by default, the system applies a 2% tip on all transactions, matching the previous 2% dev fee behavior but now user-configurable.

## Backward Compatibility

The `dev_fee_enabled` flag is preserved for backward compatibility. The tip calculation is triggered if:
- `dev_fee_enabled` is true, OR
- `dev_tip_percent > 0.0`, OR  
- `dev_tip_fixed_sol > 0.0`

This ensures existing configurations continue to work while allowing the new flexible tip system.
