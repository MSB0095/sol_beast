import { create } from 'zustand'
import { botService } from '../services/botService'

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
  
  // Dev Tip
  dev_tip_percent: number
  dev_tip_fixed_sol: number
  
  // Shyft
  shyft_api_key?: string
  shyft_graphql_url: string
}

type SettingsTab = 'dashboard' | 'configuration' | 'holdings' | 'logs' | 'newcoins' | 'trades' | 'profile'

interface SettingsStore {
  settings: Settings | null
  loading: boolean
  saving: boolean
  error: string | null
  activeTab: SettingsTab
  
  fetchSettings: () => Promise<void>
  saveSettings: (settings: Partial<Settings>) => Promise<void>
  updateSetting: <K extends keyof Settings>(key: K, value: Settings[K]) => void
  setActiveTab: (tab: SettingsTab) => void
  setError: (error: string | null) => void
}

const defaultSettings: Settings = {
  solana_ws_urls: ['wss://api.mainnet-beta.solana.com/'],
  solana_rpc_urls: ['https://api.mainnet-beta.solana.com/'],
  pump_fun_program: '6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P',
  metadata_program: 'metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s',
  tp_percent: 100,
  sl_percent: -50,
  timeout_secs: 50,
  buy_amount: 0.001,
  enable_safer_sniping: true,
  min_tokens_threshold: 30000,
  max_sol_per_token: 0.002,
  slippage_bps: 500,
  min_liquidity_sol: 0,
  max_liquidity_sol: 15,
  max_create_to_buy_secs: 5,
  max_holded_coins: 4,
  price_source: 'wss',
  rotate_rpc: true,
  rpc_rotate_interval_secs: 6000,
  max_subs_per_wss: 2,
  sub_ttl_secs: 900,
  wss_subscribe_timeout_secs: 10,
  cache_capacity: 1024,
  price_cache_ttl_secs: 30,
  bonding_curve_strict: false,
  bonding_curve_log_debounce_secs: 300,
  helius_sender_enabled: true,
  helius_sender_endpoint: 'https://sender.helius-rpc.com/fast',
  helius_min_tip_sol: 0.00001,
  helius_priority_fee_multiplier: 1.2,
  helius_use_swqos_only: true,
  helius_use_dynamic_tips: true,
  helius_confirm_timeout_secs: 1,
  dev_tip_percent: 2.0,
  dev_tip_fixed_sol: 0.0,
  shyft_api_key: undefined,
  shyft_graphql_url: 'https://programs.shyft.to/v0/graphql',
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: defaultSettings,
  loading: false,
  saving: false,
  error: null,
  activeTab: 'dashboard',
  
  fetchSettings: async () => {
    set({ loading: true, error: null })
    try {
      // Use botService which handles WASM/REST mode
      const settings = await botService.getSettings()
      set({ settings, loading: false })
    } catch (err) {
      set({ 
        settings: defaultSettings,
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to fetch settings'
      })
    }
  },
  
  saveSettings: async (updates) => {
    // Get current state using the get parameter to avoid stale state issues
    const currentState = useSettingsStore.getState()
    
    set({ saving: true, error: null })
    try {
      // Merge updates with current settings to get full settings object
      const currentSettings = currentState.settings || defaultSettings
      const fullSettings = { ...currentSettings, ...updates }
      
      // Use botService which handles WASM/REST mode
      // For WASM, we need to pass the full settings object
      await botService.updateSettings(fullSettings)
      
      set({
        settings: fullSettings,
        saving: false,
      })
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
  setError: (error) => set({ error }),
}))
