import { create } from 'zustand'
import { API_SETTINGS_URL } from '../config'

export interface Settings {
  // RPC & WebSocket
  solana_ws_urls: string[]
  solana_rpc_urls: string[]
  pump_fun_program: string
  metadata_program: string
  
  // Wallet
  wallet_keypair_path?: string
  wallet_private_key_string?: string
  
  // Trading Strategy
  tp_percent: number
  sl_percent: number
  timeout_secs: number
  buy_amount: number
  
  // Safety & Filters
  enable_safer_sniping: boolean
  min_tokens_threshold: number
  max_sol_per_token: number
  slippage_bps: number
  min_liquidity_sol: number
  max_liquidity_sol: number
  
  // Timing
  max_create_to_buy_secs: number
  
  // Position Management
  max_holded_coins: number
  
  // Price Data
  price_source: string
  
  // RPC Rotation
  rotate_rpc: boolean
  rpc_rotate_interval_secs: number
  
  // WebSocket Config
  max_subs_per_wss: number
  sub_ttl_secs: number
  wss_subscribe_timeout_secs: number
  
  // Cache
  cache_capacity: number
  price_cache_ttl_secs: number
  
  // Advanced
  bonding_curve_strict: boolean
  bonding_curve_log_debounce_secs: number
  
  // Helius
  helius_sender_enabled: boolean
  helius_api_key?: string
  helius_sender_endpoint: string
  helius_min_tip_sol: number
  helius_priority_fee_multiplier: number
  helius_use_swqos_only: boolean
  helius_use_dynamic_tips: boolean
  helius_confirm_timeout_secs: number
}

export type SettingsTab = 'dashboard' | 'configuration' | 'holdings' | 'logs' | 'newcoins' | 'trades'

interface SettingsStore {
  settings: Settings | null
  loading: boolean
  saving: boolean
  error: string | null
  activeTab: SettingsTab
  engine: 'backend' | 'wasm'
  setEngine: (engine: 'backend' | 'wasm') => void
  
  fetchSettings: () => Promise<void>
  saveSettings: (settings: Partial<Settings>) => Promise<void>
  updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void
  setActiveTab: (tab: SettingsTab) => void
  setError: (error: string | null) => void
}

// Note: we avoid populating UI with hardcoded defaults when backend is unreachable.
// The app will show a loading / error state and require the backend to provide
// authoritative settings. This prevents placeholders from appearing as "real" data.

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: null,
  loading: false,
  saving: false,
  error: null,
  activeTab: 'dashboard',
  engine: 'backend',
  
  fetchSettings: async () => {
    set({ loading: true, error: null })
    try {
      const response = await fetch(API_SETTINGS_URL)
      if (response.ok) {
        const settings = await response.json()
        set({ settings, loading: false })
      } else {
        set({ 
          settings: null,
          loading: false,
          error: 'Failed to fetch settings from backend'
        })
      }
    } catch (err) {
      set({ 
        settings: null,
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to fetch settings'
      })
    }
  },
  
  saveSettings: async (updates) => {
    set({ saving: true, error: null })
    try {
      const response = await fetch(API_SETTINGS_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(updates),
      })

      if (response.ok) {
        // Use the authoritative settings returned by backend if available
        const saved = await response.json().catch(() => null)
        set((state) => ({
          settings: saved || (state.settings ? { ...state.settings, ...updates } : null),
          saving: false,
        }))
      } else {
        const errorData = await response.json().catch(() => ({ message: 'Failed to save settings' }))
        set({ 
          saving: false,
          error: errorData.message || 'Failed to save settings'
        })
      }
    } catch (err) {
      set({ 
        saving: false,
        error: err instanceof Error ? err.message : 'Failed to save settings'
      })
    }
  },
  
  updateSetting: (key, value) => {
    set((state) => ({
      settings: state.settings 
        ? { ...state.settings, [key]: value }
        : defaultSettings
    }))
  },
  
  setActiveTab: (tab) => set({ activeTab: tab }),
  setEngine: (engine: 'backend' | 'wasm') => set({ engine }),
  setError: (error) => set({ error }),
}))
