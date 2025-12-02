# Hot-Reload Settings Feature

## Overview

The bot now supports updating settings while running (hot-reload). This allows you to adjust trading parameters without stopping and restarting the bot, improving operational efficiency.

## How It Works

### Settings Categories

Settings are divided into two categories based on whether they require a bot restart:

#### 1. **Non-Critical Settings** (Apply Immediately)
These settings affect future trades and can be updated without restarting:

- **Trading Parameters**
  - `tp_percent` - Take Profit percentage
  - `sl_percent` - Stop Loss percentage
  - `timeout_secs` - Trade timeout
  - `buy_amount` - SOL amount per buy
  - `max_holded_coins` - Maximum concurrent holdings

- **Safety & Filtering**
  - `enable_safer_sniping` - Safer sniping mode
  - `min_tokens_threshold` - Minimum token threshold
  - `max_sol_per_token` - Maximum SOL per token
  - `slippage_bps` - Slippage tolerance
  - `min_liquidity_sol` - Minimum liquidity
  - `max_liquidity_sol` - Maximum liquidity

- **Helius Configuration**
  - `helius_sender_enabled` - Enable/disable Helius
  - `helius_min_tip_sol` - Minimum tip amount
  - `helius_priority_fee_multiplier` - Fee multiplier
  - All other Helius settings

- **Advanced Settings**
  - `price_source` - Price source selection
  - `rotate_rpc` - RPC rotation
  - Cache and timeout settings

#### 2. **Critical Settings** (Require Restart)
These settings affect the bot's connection and require manual restart:

- `solana_ws_urls` - WebSocket URLs (connection to Solana)
- `pump_fun_program` - Program ID being monitored

**Why restart is needed:** The WebSocket connection is established when the bot starts. Changing the URL or program ID requires closing the old connection and opening a new one, which can only be done by restarting the bot.

## User Experience

### Updating Settings While Running

1. **Navigate to Settings**: Open the Configuration Panel
2. **Warning Banner**: You'll see a yellow warning banner:
   ```
   ⚠️ BOT IS RUNNING
   You can update settings while running. Trading parameters (TP, SL, buy amount, etc.) apply to future trades.
   ⚠️ WebSocket URL and Program ID changes require bot restart.
   ```

3. **Make Changes**: Update any settings you want
4. **Click Save**: The save button is now enabled even while running

### After Saving

#### Non-Critical Settings Updated
```
✓ Settings updated!
Trading parameters updated. Changes will apply to future trades.
```
- Changes take effect immediately
- Next trade will use the new parameters
- No action required

#### Critical Settings Updated
```
⚠️ Settings updated!
WebSocket URL or program ID changed. Please restart the bot for changes to take effect.
```
- Settings are saved but not yet active
- Manual restart required
- Old settings remain in effect until restart

## Implementation Details

### WASM Bot Logic

```rust
pub fn update_settings(&self, settings_json: &str) -> Result<(), JsValue> {
    // Parse new settings
    let settings: BotSettings = serde_json::from_str(settings_json)?;
    
    // Check if bot is running and if critical settings changed
    let (is_running, needs_restart) = {
        let state = self.state.lock()?;
        let is_running = state.running;
        
        // Detect critical changes
        let needs_restart = if is_running {
            let ws_changed = old_ws_url != new_ws_url;
            let program_changed = old_program != new_program;
            ws_changed || program_changed
        } else {
            false
        };
        
        (is_running, needs_restart)
    };
    
    // Save to localStorage
    save_settings(&settings)?;
    
    // Update in-memory settings
    let mut state = self.state.lock()?;
    state.settings = settings;
    
    // Add appropriate log entry
    if is_running && needs_restart {
        state.logs.push(warning_about_restart);
    } else if is_running {
        state.logs.push(success_message);
    }
    
    Ok(())
}
```

### Frontend UI

```typescript
const handleSave = async () => {
  await saveSettings(settings)
  
  if (isBotRunning) {
    setSuccessMessage('Settings updated! Restart bot for WebSocket/Program changes.')
  } else {
    setSuccessMessage('Settings saved successfully!')
  }
  
  setTimeout(() => setSuccessMessage(''), 5000)
}
```

## Benefits

### 1. Operational Efficiency
- Adjust trading parameters on the fly
- React to market conditions without downtime
- Fine-tune strategy based on performance

### 2. Reduced Disruption
- No need to stop monitoring
- No WebSocket reconnection overhead for minor changes
- Continuous operation during parameter adjustments

### 3. Iterative Optimization
- Test different TP/SL values quickly
- Adjust position sizing without restart
- Experiment with filtering parameters

### 4. Clear User Feedback
- Always know which settings need restart
- Immediate confirmation of changes
- Warnings in both UI and logs

## Usage Examples

### Example 1: Adjusting Take Profit During High Volatility

**Scenario**: Market is very volatile, you want to take profits earlier

1. Navigate to Settings → Trading Strategy
2. Change `tp_percent` from 100% to 50%
3. Click Save Settings
4. ✓ Next token purchase will use 50% TP
5. No restart needed

### Example 2: Switching to Premium RPC

**Scenario**: You want to use a faster RPC endpoint

1. Navigate to Settings → RPC & WebSocket Configuration
2. Change `solana_ws_urls` to your premium WebSocket
3. Click Save Settings
4. ⚠️ Warning: "Restart required"
5. Click Stop Bot
6. Click Start Bot
7. ✓ Now using premium WebSocket

### Example 3: Increasing Buy Amount

**Scenario**: You want to invest more per trade

1. Navigate to Settings → Trading Strategy
2. Change `buy_amount` from 0.001 to 0.01 SOL
3. Click Save Settings
4. ✓ Next buy will use 0.01 SOL
5. No restart needed

## Technical Notes

### localStorage Persistence

All settings updates are automatically saved to browser's localStorage, ensuring:
- Changes persist across page refreshes
- No data loss on accidental page close
- Consistent state between sessions

### Thread Safety

The implementation uses proper locking mechanisms:
```rust
let mut state = self.state.lock()?;
```
Ensures:
- No race conditions
- Consistent state
- Safe concurrent access

### Error Handling

Comprehensive error handling at every step:
- JSON parsing errors
- localStorage save failures
- Lock acquisition failures
- State inconsistencies

All errors are caught, logged, and reported to the user.

## Limitations

1. **WebSocket Reconnection**: Cannot hot-reload WebSocket URL (by design)
2. **Program Monitoring**: Cannot hot-reload program ID (by design)
3. **Active Trades**: Settings changes don't affect trades already in progress
4. **Monitor State**: The running monitor continues with its initial settings until restart

## Future Enhancements

Potential improvements (not currently implemented):

1. **Automatic Restart**: Option to auto-restart when critical settings change
2. **Settings Preview**: Show which settings need restart before saving
3. **Validation**: Real-time validation of settings before applying
4. **Rollback**: Ability to undo recent settings changes
5. **History**: Track settings change history with timestamps

## Security Considerations

- Settings are only stored locally in browser
- No transmission of sensitive data
- localStorage is domain-scoped
- HTTPS ensures secure updates on GitHub Pages

## Summary

The hot-reload settings feature provides:
- ✅ Flexibility to adjust non-critical parameters while running
- ✅ Clear indication when restart is required
- ✅ Automatic persistence of all changes
- ✅ User-friendly warnings and confirmations
- ✅ No risk of data loss or corruption

This improves the operational efficiency of the bot while maintaining safety and clarity about when manual intervention is needed.
