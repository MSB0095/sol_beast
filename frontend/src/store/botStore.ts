import { create } from 'zustand'

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

export interface LogEntry {
  timestamp: string
  level: 'info' | 'warn' | 'error'
  message: string
  details?: string
}

interface BotStore {
  status: BotStatus
  stats: BotStats | null
  error: string | null
  logs: LogEntry[]
  runningState: BotRunningState
  mode: BotMode
  pollInterval: number | null
  initializeConnection: () => void
  updateStatus: (status: BotStatus) => void
  updateStats: (stats: BotStats) => void
  setError: (error: string | null) => void
  startBot: () => Promise<void>
  stopBot: () => Promise<void>
  setMode: (mode: BotMode) => Promise<void>
  addLog: (log: LogEntry) => void
  clearLogs: () => void
}

export const useBotStore = create<BotStore>((set, get) => ({
  status: 'disconnected',
  stats: null,
  error: null,
  logs: [],
  runningState: 'stopped',
  mode: 'dry-run',
  pollInterval: null,
  
  initializeConnection: async () => {
    try {
      const response = await fetch('http://localhost:8080/health')
      if (response.ok) {
        set({ status: 'connected', error: null })
        
        // Load initial state
        try {
          const stateRes = await fetch('http://localhost:8080/bot/state')
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
        
        // Start polling for stats
        const pollStats = async () => {
          try {
            const res = await fetch('http://localhost:8080/stats')
            if (res.ok) {
              const stats = await res.json()
              set({ 
                stats,
                runningState: stats.running_state || get().runningState,
                mode: stats.mode || get().mode
              })
            }
          } catch (err) {
            console.error('Failed to fetch stats:', err)
          }
        }
        
        // Poll logs
        const pollLogs = async () => {
          try {
            const res = await fetch('http://localhost:8080/logs')
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
        
        pollStats()
        pollLogs()
        const interval = setInterval(() => {
          pollStats()
          pollLogs()
        }, 2000)
        
        set({ pollInterval: interval })
      } else {
        set({ status: 'disconnected', error: 'Backend not available' })
      }
    } catch (err) {
      set({ 
        status: 'disconnected', 
        error: err instanceof Error ? err.message : 'Connection failed' 
      })
    }
  },
  
  startBot: async () => {
    set({ runningState: 'starting' })
    try {
      const response = await fetch('http://localhost:8080/bot/start', {
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
      const response = await fetch('http://localhost:8080/bot/stop', {
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
      const response = await fetch('http://localhost:8080/bot/mode', {
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
  
  updateStatus: (status) => set({ status }),
  updateStats: (stats) => set({ stats }),
  setError: (error) => set({ error }),
}))
