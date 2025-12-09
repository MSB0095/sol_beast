// Dual-mode bot service: WASM or REST API
import { API_BASE_URL, API_DETECTED_COINS_URL } from '../config'

// Feature detection - automatically enable WASM mode on GitHub Pages
const USE_WASM = (typeof __VITE_ENV__ !== 'undefined' && __VITE_ENV__.VITE_USE_WASM === 'true') || 
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
  // Phase 4: Holdings Management
  add_holding(mint: string, amount: bigint, buy_price: number, metadata_json: string | null): void
  monitor_holdings(): Promise<string>
  build_sell_transaction(mint: string, userPubkey: string): string
  remove_holding(mint: string, profit_percent: number, reason: string): void
  test_rpc_connection(): Promise<string>
  test_ws_connection(): Promise<string>
  save_to_storage(): void
  load_from_storage(): void
}

let wasmBot: WasmBot | null = null
let wasmInitialized = false

type BotSettingsShape = {
  solana_ws_urls: string[]
  solana_rpc_urls: string[]
  pump_fun_program: string
  metadata_program: string
  tp_percent: number
  sl_percent: number
  timeout_secs: number
  buy_amount: number
  max_holded_coins: number
  slippage_bps: number
  min_tokens_threshold: number
  max_sol_per_token: number
  min_liquidity_sol: number
  max_liquidity_sol: number
  dev_tip_percent: number
  dev_tip_fixed_sol: number
  shyft_api_key: string
  shyft_graphql_url: string
  enable_safer_sniping?: boolean
}

// Force a full WASM restart and reapply sane defaults to recover from memory corruption
async function rebuildWasm(reason: string): Promise<WasmBot> {
  console.warn('Rebuilding WASM bot after critical error:', reason)
  wasmBot = null
  wasmInitialized = false
  const ok = await initWasm()
  if (!ok || !wasmBot) {
    throw new Error('Failed to reinitialize WASM bot')
  }

  // Re-seed settings with validated defaults to avoid corrupted state lingering in memory
  const defaults = await loadDefaultSettings()
  const bot = wasmBot as WasmBot
  bot.update_settings(JSON.stringify(defaults))
  return bot
}

// Check if an error is a critical WASM error that requires recovery
function isCriticalWasmError(err: unknown, errorMsg: string): boolean {
  // Check for null or undefined errors (using == to catch both)
  if (err === null || err === undefined) return true
  
  // Check for WASM panic indicators in error message
  return (
    errorMsg.includes('unreachable') || 
    errorMsg.includes('undefined') ||
    errorMsg.includes('memory access out of bounds')
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

const HARDCODED_DEFAULTS: BotSettingsShape = {
  solana_ws_urls: ["wss://api.mainnet-beta.solana.com/"],
  solana_rpc_urls: ["https://api.mainnet-beta.solana.com"],
  pump_fun_program: "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P",
  metadata_program: "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s",
  tp_percent: 100.0,
  sl_percent: -50.0,
  timeout_secs: 50,
  buy_amount: 0.001,
  max_holded_coins: 4,
  slippage_bps: 500,
  min_tokens_threshold: 30000,
  max_sol_per_token: 0.002,
  min_liquidity_sol: 0.0,
  max_liquidity_sol: 15.0,
  dev_tip_percent: 2.0,
  dev_tip_fixed_sol: 0.0,
  shyft_api_key: "",
  shyft_graphql_url: "https://programs.shyft.to/v0/graphql/?api_key=&network=mainnet-beta"
}

// Load default settings from static JSON file
async function loadDefaultSettings(): Promise<BotSettingsShape> {
  try {
    // Try loading from the public directory
    // For local development, use relative path from root
    // For production (GitHub Pages), use base path
    let response
    try {
      // Try relative path first (works in dev mode)
      response = await fetch('/bot-settings.json')
    } catch (err) {
      // If that fails, try with base path (for production)
      const basePath = typeof __VITE_ENV__ !== 'undefined' ? __VITE_ENV__.BASE_URL : '/'
      response = await fetch(`${basePath}bot-settings.json`)
    }
    
    if (!response.ok) {
      console.warn('Could not load default settings from bot-settings.json, using hardcoded defaults')
      return HARDCODED_DEFAULTS
    }
    const settings = await response.json()
    
    // Validate loaded settings before returning
    if (!validateSettings(settings)) {
      console.warn('Loaded settings from bot-settings.json are invalid, using hardcoded defaults')
      return HARDCODED_DEFAULTS
    }
    
    console.log('✓ Loaded default settings from bot-settings.json')
    return settings
  } catch (error) {
    console.warn('Failed to load default settings, using hardcoded defaults:', error)
    return HARDCODED_DEFAULTS
  }
}

// Initialize WASM if needed
async function initWasm() {
  if (!USE_WASM) return true
  if (wasmInitialized) return true
  
  try {
    console.log('Initializing WASM module...')
    
    // Clear potentially corrupted localStorage before initialization
    try {
      localStorage.removeItem('sol_beast_settings')
      localStorage.removeItem('sol_beast_state')
      localStorage.removeItem('sol_beast_holdings')
      console.log('✓ Cleared localStorage before WASM initialization')
    } catch (e) {
      console.warn('Could not clear localStorage:', e)
    }
    
    // Dynamically import WASM module
    const wasm = await import('../wasm/sol_beast_wasm')
    console.log('✓ WASM module imported successfully')
    
    // Initialize WASM module without custom memory (let wasm-bindgen handle it)
    // The custom memory approach was causing initialization issues
    await wasm.default()
    console.log('✓ WASM module initialized')
    
    // Note: wasm.init() is called automatically by #[wasm_bindgen(start)]
    // during wasm.default(), so we don't need to call it explicitly
    
    // Create bot instance (this will load from localStorage if available)
    console.log('Creating SolBeastBot instance...')
    wasmBot = new wasm.SolBeastBot()
    console.log('✓ SolBeastBot instance created')
    
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
    return USE_WASM
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
            console.log('Critical error detected, attempting full WASM rebuild...')
            try {
              const bot: WasmBot = await rebuildWasm('settings retrieval failed during start')
              settingsJson = bot.get_settings()
              console.log('✓ Successfully recovered via WASM rebuild')
            } catch (recoveryError) {
              const recoveryMsg = recoveryError instanceof Error ? recoveryError.message : String(recoveryError)
              throw new Error(`Failed to recover bot settings after rebuild: ${recoveryMsg}. Original error: ${errorMsg}`)
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
          const errorMsg = err instanceof Error ? err.message : String(err)
          
          // Check for critical WASM errors
          if (isCriticalWasmError(err, errorMsg)) {
             console.error('Critical WASM error during start, resetting instance...')
             wasmInitialized = false
             wasmBot = null
          }

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
  // MEMORY SAFETY: Implements recovery for memory access errors during mode changes
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
        const errorMsg = error instanceof Error ? error.message : String(error)
        
        // Check if this is a critical WASM error (memory access out of bounds)
        if (isCriticalWasmError(error, errorMsg)) {
          console.log('Critical WASM error detected during mode change, attempting recovery...')
          
          try {
            localStorage.removeItem('sol_beast_settings')
            localStorage.removeItem('sol_beast_state')
            localStorage.removeItem('sol_beast_holdings')
            const bot: WasmBot = await rebuildWasm('mode change memory error')
            bot.set_mode(mode)
            return { success: true, mode }
          } catch (recoveryError) {
            console.error('Recovery failed:', recoveryError)
            throw new Error(`Failed to recover from memory error. Please refresh the page. Original error: ${errorMsg}`)
          }
        }
        
        throw new Error(errorMsg)
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
  // MEMORY SAFETY: Handles memory errors gracefully and returns safe defaults
  async getStatus() {
    if (this.isWasmMode()) {
      if (!wasmBot || !wasmInitialized) {
        // Return default state if not initialized yet
        return {
          running: false,
          mode: 'dry-run'
        }
      }
      try {
        return {
          running: wasmBot.is_running(),
          mode: wasmBot.get_mode()
        }
      } catch (error) {
        // Handle memory access errors gracefully
        console.warn('Error getting WASM status:', error)
        const errorMsg = error instanceof Error ? error.message : String(error)
        
        // If it's a critical error, try to clear corrupted data
        if (isCriticalWasmError(error, errorMsg)) {
          console.log('Critical error in getStatus, clearing potentially corrupted localStorage...')
          try {
            localStorage.removeItem('sol_beast_settings')
            localStorage.removeItem('sol_beast_state')
            // Attempt a lightweight rebuild so subsequent calls have a clean instance
            await rebuildWasm('getStatus memory error')
          } catch (e) {
            console.error('Failed to clear localStorage:', e)
          }
        }
        
        // Always return a safe default state
        return {
          running: false,
          mode: 'dry-run'
        }
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
  // MEMORY SAFETY: Implements comprehensive error recovery for settings retrieval
  async getSettings() {
    if (this.isWasmMode()) {
      if (!wasmBot || !wasmInitialized) {
        // If WASM failed to init, try to return defaults so UI can work
        console.warn('WASM not initialized, returning default settings')
        const defaults = await loadDefaultSettings()
        if (defaults) return defaults
        
        // Wait a moment for initialization as a last resort
        await new Promise(resolve => setTimeout(resolve, 100))
        if (!wasmBot || !wasmInitialized) {
          throw new Error('WASM bot is not initialized and defaults could not be loaded')
        }
      }
      try {
        const json = wasmBot.get_settings()
        const settings = JSON.parse(json)
        
        // Validate settings structure
        if (!validateSettings(settings)) {
          console.warn('Retrieved settings failed validation, attempting recovery...')
          throw new Error('Settings validation failed')
        }
        
        return settings
      } catch (error) {
        console.error('Get settings error:', error)
        const errorMsg = error instanceof Error ? error.message : String(error)
        
        // Check if this is a critical WASM error
        if (isCriticalWasmError(error, errorMsg)) {
          console.log('Critical WASM error in get_settings, attempting recovery...')
          
          // Try to recover by loading defaults
          try {
            const bot: WasmBot = await rebuildWasm('get_settings memory error')
            const defaultSettings = await loadDefaultSettings()
            bot.update_settings(JSON.stringify(defaultSettings))
            console.log('✓ Recovered settings from default configuration after rebuild')
            return defaultSettings
          } catch (recoveryError) {
            console.error('Failed to load default settings during recovery:', recoveryError)
          }
          
          throw new Error(`Failed to recover bot settings: ${errorMsg}`)
        }
        
        throw new Error(errorMsg)
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
  async updateSettings(settings: unknown) {
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
      if (!wasmBot || !wasmInitialized) {
        return []
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
      if (!wasmBot || !wasmInitialized) {
        return []
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
      throw new Error('Transaction building is only supported in WASM mode. Please enable WASM mode to build and submit transactions.')
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
  },

  // Phase 4: Holdings Management Methods

  // Add a holding after successful purchase
  addHolding(mint: string, amount: bigint, buyPrice: number, metadata?: unknown) {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      const metadataJson = metadata ? JSON.stringify(metadata) : null
      return wasmBot.add_holding(mint, amount, buyPrice, metadataJson)
    } else {
      // REST API mode: POST to /holdings endpoint
      throw new Error('Holdings management in REST API mode not yet implemented')
    }
  },

  // Monitor holdings for TP/SL/timeout conditions
  async monitorHoldings() {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = await wasmBot.monitor_holdings()
        return JSON.parse(json)
      } catch (error: unknown) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      // REST API mode: GET /monitor endpoint
      throw new Error('Holdings monitoring in REST API mode not yet implemented')
    }
  },

  // Build sell transaction for a holding
  buildSellTransaction(mint: string, userPubkey: string) {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      try {
        const json = wasmBot.build_sell_transaction(mint, userPubkey)
        return JSON.parse(json)
      } catch (error: unknown) {
        throw new Error(error instanceof Error ? error.message : String(error))
      }
    } else {
      throw new Error('Transaction building is only supported in WASM mode')
    }
  },

  // Remove a holding after successful sell
  removeHolding(mint: string, profitPercent: number, reason: string) {
    if (this.isWasmMode()) {
      if (!wasmBot) {
        throw new Error('WASM bot is not initialized')
      }
      return wasmBot.remove_holding(mint, profitPercent, reason)
    } else {
      // REST API mode: DELETE /holdings/:mint endpoint
      throw new Error('Holdings removal in REST API mode not yet implemented')
    }
  }
}

// Initialize on module load
botService.init().then(() => {
  console.log(`Bot service initialized (${botService.isWasmMode() ? 'WASM' : 'REST API'} mode)`)
})
