// Dual-mode bot service: WASM or REST API
import { API_BASE_URL } from '../config'

// Feature detection
const USE_WASM = import.meta.env.VITE_USE_WASM === 'true' || 
                 window.location.hostname.includes('github.io')

let wasmBot: any = null
let wasmInitialized = false

// Initialize WASM if needed
async function initWasm() {
  if (!USE_WASM || wasmInitialized) return
  
  try {
    // Dynamically import WASM module
    const wasm = await import('../wasm/sol_beast_wasm')
    await wasm.default() // Initialize WASM
    await wasm.init() // Call our init function
    wasmBot = new wasm.SolBeastBot()
    wasmInitialized = true
    console.log('✓ WASM bot initialized')
  } catch (error) {
    console.warn('WASM initialization failed, falling back to REST API:', error)
    // Fall back to REST API
    return false
  }
  
  return true
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
      try {
        // Ensure settings are synced before starting
        // Get current settings from WASM bot
        let settingsJson
        try {
          settingsJson = wasmBot.get_settings()
        } catch (err) {
          console.error('Failed to get WASM settings:', err)
          throw new Error(`Failed to get bot settings: ${err instanceof Error ? err.message : String(err)}`)
        }
        
        // Parse and validate settings
        let settings
        try {
          settings = JSON.parse(settingsJson)
        } catch (parseError) {
          throw new Error(`Failed to parse bot settings: ${parseError instanceof Error ? parseError.message : String(parseError)}`)
        }
        
        // Validate required settings
        if (!settings.solana_ws_urls || settings.solana_ws_urls.length === 0) {
          throw new Error('No WebSocket URL configured. Please configure settings before starting the bot.')
        }
        if (!settings.solana_rpc_urls || settings.solana_rpc_urls.length === 0) {
          throw new Error('No RPC URL configured. Please configure settings before starting the bot.')
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

  // Test RPC connection (WASM only)
  async testRpcConnection() {
    if (this.isWasmMode()) {
      return wasmBot.test_rpc_connection()
    }
    throw new Error('RPC test only available in WASM mode')
  },

  // Test WebSocket connection (WASM only)
  async testWsConnection() {
    if (this.isWasmMode()) {
      return wasmBot.test_ws_connection()
    }
    throw new Error('WebSocket test only available in WASM mode')
  },

  // Save to storage (WASM only)
  async saveToStorage() {
    if (this.isWasmMode()) {
      return wasmBot.save_to_storage()
    }
  },

  // Load from storage (WASM only)
  async loadFromStorage() {
    if (this.isWasmMode()) {
      return wasmBot.load_from_storage()
    }
  }
}

// Initialize on module load
botService.init().then(() => {
  console.log(`Bot service initialized (${botService.isWasmMode() ? 'WASM' : 'REST API'} mode)`)
})
