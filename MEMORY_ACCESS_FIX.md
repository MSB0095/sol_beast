# Memory Access Out of Bounds Fix

## Problem Description

Users were experiencing "memory access out of bounds" errors when:
1. Starting the bot
2. Changing bot mode (dry-run/real)
3. Getting bot settings

Error logs showed:
```
error: Failed to start bot - memory access out of bounds
error: Failed to change bot mode - memory access out of bounds
error: Failed to get bot settings - memory access out of bounds
```

## Root Cause Analysis

The "memory access out of bounds" error in WASM typically occurs when:
1. **Corrupted localStorage data**: Settings saved in a previous session may have an incompatible structure
2. **Missing fields**: New fields added to `BotSettings` struct aren't present in old localStorage data
3. **Deserialization failures**: When serde_json fails to deserialize due to structural mismatches, it can trigger WASM panics

## Solution Implemented

### 1. Backend (Rust WASM) - Resilient Deserialization

**File: `sol_beast_wasm/src/lib.rs`**
- Added `#[serde(default)]` attribute to the entire `BotSettings` struct
- This ensures that if any field is missing during deserialization, it will use the default value from the `Default` impl instead of failing

```rust
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]  // <- Added this
pub struct BotSettings {
    // ... all fields ...
}
```

**File: `sol_beast_core/src/wasm/storage.rs`**
- Improved `load_settings()` function to gracefully handle deserialization errors
- When deserialization fails:
  1. Log a warning to the browser console
  2. Clear the corrupted data from localStorage
  3. Return `None` so the bot will use defaults

```rust
match serde_json::from_str(&json) {
    Ok(settings) => Ok(Some(settings)),
    Err(e) => {
        // Log the error via console
        web_sys::console::warn_1(&format!("Failed to deserialize settings...").into());
        
        // Clear the corrupted data
        let _ = storage.remove_item(SETTINGS_KEY);
        
        // Return None so defaults will be used
        Ok(None)
    }
}
```

### 2. Frontend (TypeScript) - Error Recovery

**File: `frontend/src/services/botService.ts`**

#### Added "memory access out of bounds" to critical error detection:
```typescript
function isCriticalWasmError(err: unknown, errorMsg: string): boolean {
  return (
    errorMsg.includes('unreachable') || 
    errorMsg.includes('undefined') ||
    errorMsg.includes('memory access out of bounds')  // <- Added this
  )
}
```

#### Added recovery logic to `setMode()`:
- Detects critical WASM errors
- Reinitializes the WASM module
- Retries the operation

#### Added recovery logic to `getSettings()`:
- Detects critical WASM errors
- Reinitializes the WASM module
- Retries the operation

Both functions now follow this pattern:
1. Try the operation
2. If critical WASM error detected:
   - Reset wasmBot to null
   - Call initWasm() to reinitialize
   - Recursively retry the operation
3. If recovery fails, throw a clear error message

## Benefits

1. **Backwards Compatibility**: Old localStorage data won't crash the app
2. **Automatic Recovery**: The app can recover from corrupted state automatically
3. **Clear Error Messages**: Users see helpful error messages instead of cryptic WASM errors
4. **No Data Loss**: User settings are preserved when possible, or reset cleanly to defaults
5. **Self-Healing**: After recovery, the app saves new, properly structured settings

## Testing Recommendations

To verify the fix works:

1. **Simulate corrupted localStorage**:
   ```javascript
   // In browser console:
   localStorage.setItem('sol_beast_settings', '{"invalid": "json structure"}');
   ```
   Then refresh the page and verify the bot initializes correctly with defaults.

2. **Simulate missing fields**:
   ```javascript
   // In browser console:
   localStorage.setItem('sol_beast_settings', JSON.stringify({
     solana_ws_urls: ["wss://api.mainnet-beta.solana.com/"],
     // Missing all other required fields
   }));
   ```
   Then refresh and verify the bot uses defaults for missing fields.

3. **Test mode changes**:
   - Start the bot
   - Change mode from dry-run to real and back
   - Verify no errors occur

4. **Test settings retrieval**:
   - Open RPC configuration modal
   - Verify settings load correctly
   - Make changes and save
   - Verify changes persist

## Backward Compatibility

The changes are fully backward compatible:
- Old localStorage data will be cleaned up automatically if corrupted
- Valid old data will be upgraded transparently with default values for new fields
- No user action required

## Future Improvements

Consider adding:
1. Version field to settings to track schema changes
2. Migration logic for major schema changes
3. Settings validation on save
4. User-visible notification when settings are reset
5. Option to export/import settings for backup
