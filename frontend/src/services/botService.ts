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
      return wasmBot.start()
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/start`, { method: 'POST' })
      return response.json()
    }
  },

  // Stop bot
  async stop() {
    if (this.isWasmMode()) {
      return wasmBot.stop()
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/stop`, { method: 'POST' })
      return response.json()
    }
  },

  // Set mode
  async setMode(mode: 'dry-run' | 'real') {
    if (this.isWasmMode()) {
      return wasmBot.set_mode(mode)
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/mode`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode })
      })
      return response.json()
    }
  },

  // Get status
  async getStatus() {
    if (this.isWasmMode()) {
      return {
        running: wasmBot.is_running(),
        mode: wasmBot.get_mode()
      }
    } else {
      const response = await fetch(`${API_BASE_URL}/bot/state`)
      return response.json()
    }
  },

  // Get settings
  async getSettings() {
    if (this.isWasmMode()) {
      const json = wasmBot.get_settings()
      return JSON.parse(json)
    } else {
      const response = await fetch(`${API_BASE_URL}/settings`)
      return response.json()
    }
  },

  // Update settings
  async updateSettings(settings: any) {
    if (this.isWasmMode()) {
      return wasmBot.update_settings(JSON.stringify(settings))
    } else {
      const response = await fetch(`${API_BASE_URL}/settings`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings)
      })
      return response.json()
    }
  },

  // Get logs
  async getLogs() {
    if (this.isWasmMode()) {
      const json = wasmBot.get_logs()
      return JSON.parse(json)
    } else {
      const response = await fetch(`${API_BASE_URL}/logs`)
      return response.json()
    }
  },

  // Get holdings
  async getHoldings() {
    if (this.isWasmMode()) {
      const json = wasmBot.get_holdings()
      return JSON.parse(json)
    } else {
      const response = await fetch(`${API_BASE_URL}/holdings`)
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
