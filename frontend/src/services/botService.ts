// Dual-mode bot service: WASM or REST API
import { API_BASE_URL, API_DETECTED_COINS_URL } from '../config'

// Feature detection - automatically enable WASM mode on GitHub Pages
const USE_WASM = import.meta.env.VITE_USE_WASM === 'true' || 
                 window.location.hostname.endsWith('.github.io')

// TypeScript interface for WASM bot methods
interface WasmBot {
  get_settings(): string
  update_settings(settings: string): void
  start(): void
  stop(): void
  is_running(): boolean
  get_mode(): string
  set_mode(mode: string): void
  get_logs(): string
  get_holdings(): string
  get_detected_tokens(): string
  build_buy_transaction(mint: string, userPubkey: string): string
  test_rpc_connection(): Promise<string>
  test_ws_connection(): Promise<string>
  save_to_storage(): void
  load_from_storage(): void
}

let wasmBot: WasmBot | null = null
let wasmInitialized = false

// Check if an error is a critical WASM error that requires recovery
function isCriticalWasmError(err: unknown, errorMsg: string): boolean {
  // Check for null or undefined errors (using == to catch both)
  if (err === null || err === undefined) return true
  
  // Check for WASM panic indicators in error message
  return (
    errorMsg.includes('unreachable') || 
    errorMsg.includes('undefined')
  )
}

// Validate bot settings structure
function validateSettings(settings: unknown): boolean {
  if (!settings || typeof settings !== 'object') return false
  
  const s = settings as Record<string, unknown>
  
  // Validate required array fields
  const hasValidArrays = (
    Array.isArray(s.solana_ws_urls) &&
    s.solana_ws_urls.length > 0 &&
    Array.isArray(s.solana_rpc_urls) &&
    s.solana_rpc_urls.length > 0
  )
  
  // Validate required string fields
  const hasValidStrings = (
    typeof s.pump_fun_program === 'string' &&
    s.pump_fun_program.length > 0 &&
    typeof s.metadata_program === 'string' &&
    s.metadata_program.length > 0
  )
  
  return hasValidArrays && hasValidStrings
}

// Load default settings from static JSON file
async function loadDefaultSettings() {
  try {
    // Determine the base path for the static file
    // In production (GitHub Pages), this needs to include the repo name
    const basePath = import.meta.env.BASE_URL || '/'
    const response = await fetch(`${basePath}bot-settings.json`)
    if (!response.ok) {
      console.warn('Could not load default settings from bot-settings.json')
      return null
    }
    const settings = await response.json()
    
    // Validate loaded settings before returning
    if (!validateSettings(settings)) {
      console.warn('Loaded settings from bot-settings.json are invalid')
      return null
    }
    
    console.log('✓ Loaded default settings from bot-settings.json')
    return settings
  } catch (error) {
    console.warn('Failed to load default settings:', error)
    return null
  }
}

// Initialize WASM if needed
async function initWasm() {
  if (!USE_WASM) return true
  if (wasmInitialized) return true
  
  try {
    console.log('Initializing WASM module...')
    // Dynamically import WASM module
    const wasm = await import('../wasm/sol_beast_wasm')
    
    // Initialize WASM (this calls wasm-bindgen initialization)
    await wasm.default()
    
    // Note: wasm.init() is called automatically by #[wasm_bindgen(start)]
    // during wasm.default(), so we don't need to call it explicitly
    
    // Create bot instance (this will load from localStorage if available)
    wasmBot = new wasm.SolBeastBot()
    
    // Check if settings are present and valid, if not, try to load from static file
    try {
      const currentSettings = wasmBot.get_settings()
      const settings = JSON.parse(currentSettings)
      
      // Validate settings structure
      if (!validateSettings(settings)) {
        console.log('Settings appear invalid or uninitialized, loading defaults...')
        const defaultSettings = await loadDefaultSettings()
        if (defaultSettings) {
          wasmBot.update_settings(JSON.stringify(defaultSettings))
          console.log('✓ Applied default settings')
        }
      }
    } catch (settingsError) {
      console.warn('Could not verify settings, attempting to load defaults:', settingsError)
      const defaultSettings = await loadDefaultSettings()
      if (defaultSettings) {
        try {
          wasmBot.update_settings(JSON.stringify(defaultSettings))
          console.log('✓ Applied default settings after error')
        } catch (updateError) {
          console.error('Failed to apply default settings:', updateError)
        }
      }
    }
    
    wasmInitialized = true
    console.log('✓ WASM bot initialized successfully')
    return true
  } catch (error) {
    console.error('WASM initialization failed:', error)
    // Reset state on failure
    wasmBot = null
    wasmInitialized = false
    // Fall back to REST API
    return false
  }
}

// Bot Service Interface
export const botService = {
  // Initialize the bot
  async init() {
    if (USE_WASM) {
      return await initWasm()
    }
    return true // REST API always available
  },

  // Check if using WASM mode
  isWasmMode() {
    return USE_WASM && wasmInitialized
  },

  // Start bot
  async start() {
    if (this.isWasmMode()) {
      // Ensure initialization is complete
      if (!wasmInitialized || !wasmBot) {
        console.log('WASM not fully initialized, attempting to initialize...')
        const success = await initWasm()
        if (!success || !wasmBot) {
          throw new Error('WASM bot initialization failed. Please check browser console for details.')
        }
      }
      
      try {
        // Ensure settings are synced before starting
        // Get current settings from WASM bot
        let settingsJson
        try {
          settingsJson = wasmBot.get_settings()
        } catch (err) {
          console.error('Failed to get WASM settings:', err)
          const errorMsg = err instanceof Error ? err.message : String(err)
          
          // Attempt recovery for critical errors (WASM panics, uninitialized state, etc.)
          if (isCriticalWasmError(err, errorMsg)) {
            console.log('Critical error detected, attempting recovery by loading defaults...')
            const defaultSettings = await loadDefaultSettings()
            if (defaultSettings) {
              try {
                wasmBot.update_settings(JSON.stringify(defaultSettings))
                settingsJson = wasmBot.get_settings()
                console.log('✓ Successfully recovered with default settings')
              } catch (recoveryError) {
                const recoveryMsg = recoveryError instanceof Error ? recoveryError.message : String(recoveryError)
                throw new Error(`Failed to recover bot settings: ${recoveryMsg}. Original error: ${errorMsg}`)
              }
            } else {
              throw new Error(`Failed to get bot settings and could not load defaults. Please refresh the page. Error: ${errorMsg}`)
            }
          } else {
            throw new Error(`Failed to get bot settings: ${errorMsg}`)
          }
        }
        
        // Parse and validate settings
        let settings
        try {
          settings = JSON.parse(settingsJson)
        } catch (parseError) {
          throw new Error(`Failed to parse bot settings: ${parseError instanceof Error ? parseError.message : String(parseError)}`)
        }
        
        // Validate required settings using helper function
        if (!validateSettings(settings)) {
          throw new Error('Invalid bot settings. Please configure WebSocket and RPC URLs before starting the bot.')
        }
        
        // Start the bot
        try {
          wasmBot.start()
        } catch (err) {
          console.error('Failed to start WASM bot:', err)
          // Re-throw to preserve error information
          if (err instanceof Error) {
            throw err
          }
          throw new Error(String(err))
        }
        return { success: true }
      } catch (error) {
        console.error('Bot start error:', error)
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/start`, { method: 'POST' })
      if (!response.ok) {
        try {
          const error = await response.json()
          throw new Error(error.message || 'Failed to start bot')
        } catch {
          throw new Error('Failed to start bot')
        }
      }
      return response.json()
    }
  },

  // Stop bot
  async stop() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        wasmBot.stop()
        return { success: true }
      } catch (error) {
        console.error('Bot stop error:', error)
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/stop`, { method: 'POST' })
      if (!response.ok) {
        try {
          const error = await response.json()
          throw new Error(error.message || 'Failed to stop bot')
        } catch {
          throw new Error('Failed to stop bot')
        }
      }
      return response.json()
    }
  },

  // Set mode
  async setMode(mode: 'dry-run' | 'real') {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        wasmBot.set_mode(mode)
        return { success: true, mode }
      } catch (error) {
        console.error('Set mode error:', error)
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/mode`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode })
      })
      if (!response.ok) {
        try {
          const error = await response.json()
          throw new Error(error.message || 'Failed to set mode')
        } catch {
          throw new Error('Failed to set mode')
        }
      }
      return response.json()
    }
  },

  // Get status
  async getStatus() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        return {
          running: wasmBot.is_running(),
          mode: wasmBot.get_mode()
        }
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/state`)
      if (!response.ok) {
        throw new Error('Failed to fetch bot status')
      }
      return response.json()
    }
  },

  // Get settings
  async getSettings() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = wasmBot.get_settings()
        return JSON.parse(json)
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/settings`)
      if (!response.ok) {
        throw new Error('Failed to fetch settings')
      }
      return response.json()
    }
  },

  // Update settings
  async updateSettings(settings: any) {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        wasmBot.update_settings(JSON.stringify(settings))
        return { success: true }
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/settings`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings)
      })
      if (!response.ok) {
        try {
          const error = await response.json()
          throw new Error(error.message || 'Failed to update settings')
        } catch {
          throw new Error('Failed to update settings')
        }
      }
      return response.json()
    }
  },

  // Get logs
  async getLogs() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = wasmBot.get_logs()
        return JSON.parse(json)
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/logs`)
      if (!response.ok) {
        throw new Error('Failed to fetch logs')
      }
      return response.json()
    }
  },

  // Get holdings
  async getHoldings() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = wasmBot.get_holdings()
        return JSON.parse(json)
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/holdings`)
      if (!response.ok) {
        throw new Error('Failed to fetch holdings')
      }
      return response.json()
    }
  },

  // Get detected tokens (Phase 2.5 feature)
  async getDetectedTokens() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = wasmBot.get_detected_tokens()
        return JSON.parse(json)
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      const response = await fetch(`${API_DETECTED_COINS_URL}`)
      if (!response.ok) {
        throw new Error('Failed to fetch detected tokens')
      }
      return response.json()
    }
  },
  
  // Build buy transaction (Phase 3.3 feature - WASM only for now)
  buildBuyTransaction(mint: string, userPubkey: string) {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = wasmBot.build_buy_transaction(mint, userPubkey)
        return JSON.parse(json)
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      throw new Error('Transaction building is only supported in WASM mode')
    }
  },

  // Test RPC connection (WASM only)
  async testRpcConnection() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      return wasmBot.test_rpc_connection()
    }
    throw new Error('RPC test only available in WASM mode')
  },

  // Test WebSocket connection (WASM only)
  async testWsConnection() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      return wasmBot.test_ws_connection()
    }
    throw new Error('WebSocket test only available in WASM mode')
  },

  // Save to storage (WASM only)
  async saveToStorage() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      return wasmBot.save_to_storage()
    }
  },

  // Load from storage (WASM only)
  async loadFromStorage() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      return wasmBot.load_from_storage()
    }
  }
}

// Initialize on module load
botService.init().then(() => {
  console.log(`Bot service initialized (${botService.isWasmMode() ? 'WASM' : 'REST API'} mode)`)
})
