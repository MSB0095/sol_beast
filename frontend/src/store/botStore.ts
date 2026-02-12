import { create } from 'zustand'
import {
  API_HEALTH_URL,
  API_BOT_STATE_URL,
  API_STATS_URL,
  API_LOGS_URL,
  API_BOT_START_URL,
  API_BOT_STOP_URL,
  API_BOT_MODE_URL,
} from '../config'
import { API_DETECTED_COINS_URL } from '../config'

export type BotStatus = 'connected' | 'disconnected' | 'error'
export type BotRunningState = 'running' | 'stopped' | 'starting' | 'stopping'
export type BotMode = 'dry-run' | 'real'

export interface Holding {
  mint: string
  amount: number
  buy_price: number
  buy_time: string
  metadata?: {
    name?: string
    symbol?: string
    description?: string
    image?: string
  }
  onchain?: {
    name?: string
    symbol?: string
    uri?: string
    seller_fee_basis_points?: number
  }
  onchain_raw?: number[]
}

export interface BotStats {
  total_buys: number
  total_sells: number
  total_profit: number
  current_holdings: Holding[]
  uptime_secs: number
  last_activity: string
  running_state?: BotRunningState
  mode?: BotMode
}

export interface HistoricalDataPoint {
  timestamp: number
  profit: number
  trades: number
  holdings: number
}

export interface LogEntry {
  timestamp: string
  level: 'info' | 'warn' | 'error'
  message: string
  details?: string
}

export interface PriceUpdate {
  price: number
  profit_percent: number
  pnl_sol: number
  buy_price: number
  amount: number
}

interface BotStore {
  status: BotStatus
  stats: BotStats | null
  error: string | null
  logs: LogEntry[]
  runningState: BotRunningState
  mode: BotMode
  pollInterval: number | null
  historicalData: HistoricalDataPoint[]
  lastStatUpdate: number
  detectedCoins: any[]
  totalDetectedCoins: number
  prices: Record<string, PriceUpdate>
  ws: WebSocket | null
  initializeConnection: () => Promise<void>
  updateStatus: (status: BotStatus) => void
  updateStats: (stats: BotStats) => void
  setError: (error: string | null) => void
  startBot: () => Promise<void>
  stopBot: () => Promise<void>
  setMode: (mode: BotMode) => Promise<void>
  addLog: (log: LogEntry) => void
  clearLogs: () => void
  cleanup: () => void
  fetchStats: () => Promise<void>
}

export const useBotStore = create<BotStore>((set, get) => ({
  status: 'disconnected',
  stats: null,
  error: null,
  logs: [],
  runningState: 'stopped',
  mode: 'dry-run',
  pollInterval: null,
  historicalData: [],
  lastStatUpdate: 0,
  detectedCoins: [],
  totalDetectedCoins: 0,
  prices: {},
  ws: null,
  
  initializeConnection: async () => {
    try {
      const response = await fetch(API_HEALTH_URL)
      if (response.ok) {
        set({ status: 'connected', error: null })
        
        // Load initial state
        try {
          const stateRes = await fetch(API_BOT_STATE_URL)
          if (stateRes.ok) {
            const stateData = await stateRes.json()
            set({ 
              runningState: stateData.running_state || 'stopped',
              mode: stateData.mode || 'dry-run'
            })
          }
        } catch (err) {
          console.error('Failed to fetch bot state:', err)
        }
        
        // Start polling for stats with proper interval
        const pollStats = async () => {
          try {
            const res = await fetch(API_STATS_URL)
            if (res.ok) {
              const stats = await res.json()
              const now = Date.now()
              
              // Add to historical data if enough time has passed (500ms minimum)
              set((state) => {
                const shouldAddToHistory = now - state.lastStatUpdate > 500
                
                if (shouldAddToHistory) {
                  const newHistoricalData = [
                    ...state.historicalData,
                    {
                      timestamp: now,
                      profit: stats.total_profit || 0,
                      trades: (stats.total_buys || 0) + (stats.total_sells || 0),
                      holdings: (stats.current_holdings || []).length,
                    }
                  ].slice(-100) // Keep last 100 data points
                  
                  return {
                    stats,
                    historicalData: newHistoricalData,
                    lastStatUpdate: now,
                    runningState: stats.running_state || state.runningState,
                    mode: stats.mode || state.mode
                  }
                }
                
                return {
                  stats,
                  runningState: stats.running_state || state.runningState,
                  mode: stats.mode || state.mode
                }
              })
            }
          } catch (err) {
            console.error('Failed to fetch stats:', err)
          }
        }
        
        // Poll logs
        const pollLogs = async () => {
          try {
            const res = await fetch(API_LOGS_URL)
            if (res.ok) {
              const logsData = await res.json()
              if (logsData.logs && Array.isArray(logsData.logs)) {
                set({ logs: logsData.logs })
              }
            }
          } catch (err) {
            console.error('Failed to fetch logs:', err)
          }
        }
        // Poll detected coins
        const pollDetectedCoins = async () => {
          try {
            const res = await fetch(API_DETECTED_COINS_URL)
            if (res.ok) {
              const data = await res.json()
              // New format: { coins: [...], total: N }
              if (data && typeof data.total === 'number') {
                set({ detectedCoins: data.coins || [], totalDetectedCoins: data.total })
              } else if (Array.isArray(data)) {
                // Legacy fallback
                set({ detectedCoins: data })
              }
            }
          } catch (err) {
            console.error('Failed to fetch detected coins:', err)
          }
        }
        
        // Initial poll
        await pollStats()
        await pollLogs()
        await pollDetectedCoins()
        
        // Set up polling interval - 2 seconds is reasonable for dashboard updates
        const interval = setInterval(() => {
          pollStats()
          pollLogs()
          pollDetectedCoins()
        }, 2000)
        
        set({ pollInterval: interval as unknown as number })

        // Initialize WebSocket connection
        try {
          // Construct WS URL from API_HEALTH_URL (http://host:port/api/health -> ws://host:port/api/ws)
          const wsUrl = API_HEALTH_URL.replace('http', 'ws').replace('/health', '/ws')
          const ws = new WebSocket(wsUrl)
          
          ws.onopen = () => {
             console.log('WebSocket connected')
          }
          
          ws.onmessage = (event) => {
            try {
              const msg = JSON.parse(event.data)
              
              if (msg.type === 'initial') {
                 set({ 
                   detectedCoins: msg.detected_coins || [],
                   totalDetectedCoins: msg.total_detected_coins || (msg.detected_coins || []).length,
                   stats: {
                     ...get().stats!,
                     current_holdings: msg.holdings || []
                   }
                  })
              } else if (msg.type === 'detected-coin') {
                 set(state => ({
                   detectedCoins: [msg.coin, ...state.detectedCoins].slice(0, 300),
                   totalDetectedCoins: msg.total_detected_coins || state.totalDetectedCoins + 1
                 }))
              } else if (msg.type === 'price-update') {
                 set(state => ({
                   prices: {
                     ...state.prices,
                     [msg.mint]: {
                       price: msg.price,
                       profit_percent: msg.profit_percent || 0,
                       pnl_sol: msg.pnl_sol || 0,
                       buy_price: msg.buy_price || 0,
                       amount: msg.amount || 0
                     }
                   }
                 }))
              } else if (msg.type === 'holding-update') {
                 set(state => ({
                   stats: {
                      ...state.stats!,
                      current_holdings: msg.holdings
                   }
                 }))
              }
            } catch (e) {
              console.error('Failed to parse WS message:', e)
            }
          }
          
          ws.onclose = () => {
            console.log('WebSocket disconnected')
            // Reconnection logic could go here if needed, but for now rely on re-init
          }
          
          set({ ws })
        } catch (e) {
          console.error('WebSocket init failed:', e)
        }

      } else {
        set({ status: 'disconnected', error: 'Backend not available' })
        // Stop polling if interval exists
        const current = get()
        if (current.pollInterval !== null) {
          clearInterval(current.pollInterval)
          set({ pollInterval: null })
        }
      }
    } catch (err) {
      set({ 
        status: 'disconnected', 
        error: err instanceof Error ? err.message : 'Connection failed' 
      })
      // Stop polling if interval exists
      const current = get()
      if (current.pollInterval !== null) {
        clearInterval(current.pollInterval)
        set({ pollInterval: null })
      }
    }
  },
  
  startBot: async () => {
    set({ runningState: 'starting' })
    try {
      const response = await fetch(API_BOT_START_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
      })
      if (response.ok) {
        set({ runningState: 'running', error: null })
        get().addLog({
          timestamp: new Date().toISOString(),
          level: 'info',
          message: 'Bot started successfully'
        })
      } else {
        const error = await response.json()
        set({ runningState: 'stopped', error: error.message || 'Failed to start bot' })
        get().addLog({
          timestamp: new Date().toISOString(),
          level: 'error',
          message: 'Failed to start bot',
          details: error.message
        })
      }
    } catch (err) {
      set({ runningState: 'stopped', error: 'Failed to start bot' })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'error',
        message: 'Failed to start bot',
        details: err instanceof Error ? err.message : 'Unknown error'
      })
    }
  },
  
  stopBot: async () => {
    set({ runningState: 'stopping' })
    try {
      const response = await fetch(API_BOT_STOP_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
      })
      if (response.ok) {
        set({ runningState: 'stopped', error: null })
        get().addLog({
          timestamp: new Date().toISOString(),
          level: 'info',
          message: 'Bot stopped successfully'
        })
      } else {
        const error = await response.json()
        set({ error: error.message || 'Failed to stop bot' })
        get().addLog({
          timestamp: new Date().toISOString(),
          level: 'error',
          message: 'Failed to stop bot',
          details: error.message
        })
      }
    } catch (err) {
      set({ error: 'Failed to stop bot' })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'error',
        message: 'Failed to stop bot',
        details: err instanceof Error ? err.message : 'Unknown error'
      })
    }
  },
  
  setMode: async (mode: BotMode) => {
    try {
      const response = await fetch(API_BOT_MODE_URL, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ mode }),
      })
      if (response.ok) {
        set({ mode, error: null })
        get().addLog({
          timestamp: new Date().toISOString(),
          level: 'info',
          message: `Bot mode changed to ${mode}`
        })
      } else {
        const error = await response.json()
        set({ error: error.message || 'Failed to change mode' })
        get().addLog({
          timestamp: new Date().toISOString(),
          level: 'error',
          message: 'Failed to change bot mode',
          details: error.message
        })
      }
    } catch (err) {
      set({ error: 'Failed to change mode' })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'error',
        message: 'Failed to change bot mode',
        details: err instanceof Error ? err.message : 'Unknown error'
      })
    }
  },
  
  addLog: (log: LogEntry) => {
    set((state) => ({
      logs: [log, ...state.logs].slice(0, 100) // Keep last 100 logs
    }))
  },
  
  clearLogs: () => set({ logs: [] }),
  
  cleanup: () => {
    const state = get()
    if (state.pollInterval !== null) {
      clearInterval(state.pollInterval)
    }
    set({ 
      pollInterval: null, 
      status: 'disconnected',
      historicalData: [],
      lastStatUpdate: 0
    })
  },
  
  fetchStats: async () => {
    try {
      const res = await fetch(API_STATS_URL)
      if (res.ok) {
        const stats = await res.json()
        set({ stats })
      }
    } catch (err) {
      console.error('Failed to fetch stats:', err)
    }
  },
  
  updateStatus: (status) => set({ status }),
  updateStats: (stats) => set({ stats }),
  setError: (error) => set({ error }),
}))
