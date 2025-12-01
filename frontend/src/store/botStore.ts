import { create } from 'zustand'
import {
  API_HEALTH_URL,
  API_BOT_STATE_URL,
  API_STATS_URL,
  API_LOGS_URL,
} from '../config'
import { API_DETECTED_COINS_URL } from '../config'
import { botService } from '../services/botService'

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
  
  initializeConnection: async () => {
    try {
      // Check if we're in WASM mode
      const isWasm = botService.isWasmMode()
      
      if (isWasm) {
        // In WASM mode, we're always "connected"
        set({ status: 'connected', error: null })
        
        // Load initial state from WASM
        try {
          const status = await botService.getStatus()
          set({ 
            runningState: status.running ? 'running' : 'stopped',
            mode: status.mode || 'dry-run'
          })
        } catch (err) {
          console.error('Failed to get WASM bot state:', err)
        }
        
        // In WASM mode, we use local state instead of polling
        // The stats will be managed by the WASM bot internally
        const pollWasmStats = async () => {
          try {
            const status = await botService.getStatus()
            const holdings = await botService.getHoldings()
            const logs = await botService.getLogs()
            
            const now = Date.now()
            
            // Create stats object from WASM data
            const stats = {
              total_buys: 0,
              total_sells: 0,
              total_profit: 0,
              current_holdings: holdings || [],
              uptime_secs: 0,
              last_activity: new Date().toISOString(),
              running_state: status.running ? 'running' : 'stopped',
              mode: status.mode
            }
            
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
                ].slice(-100)
                
                return {
                  stats,
                  logs: logs || state.logs,
                  historicalData: newHistoricalData,
                  lastStatUpdate: now,
                  runningState: stats.running_state as any || state.runningState,
                  mode: stats.mode as any || state.mode
                }
              }
              
              return {
                stats,
                logs: logs || state.logs,
                runningState: stats.running_state as any || state.runningState,
                mode: stats.mode as any || state.mode
              }
            })
          } catch (err) {
            console.error('Failed to fetch WASM stats:', err)
          }
        }
        
        // Initial poll
        await pollWasmStats()
        
        // Set up polling interval
        const interval = setInterval(() => {
          pollWasmStats()
        }, 2000)
        
        set({ pollInterval: interval as unknown as number })
      } else {
        // REST API mode - original behavior
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
                const coins = await res.json()
                set({ detectedCoins: coins })
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
        } else {
          set({ status: 'disconnected', error: 'Backend not available' })
          // Stop polling if interval exists
          const current = get()
          if (current.pollInterval !== null) {
            clearInterval(current.pollInterval)
            set({ pollInterval: null })
          }
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
      await botService.start()
      set({ runningState: 'running', error: null })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'info',
        message: 'Bot started successfully'
      })
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to start bot'
      set({ runningState: 'stopped', error: errorMsg })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'error',
        message: 'Failed to start bot',
        details: errorMsg
      })
    }
  },
  
  stopBot: async () => {
    set({ runningState: 'stopping' })
    try {
      await botService.stop()
      set({ runningState: 'stopped', error: null })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'info',
        message: 'Bot stopped successfully'
      })
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to stop bot'
      set({ error: errorMsg })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'error',
        message: 'Failed to stop bot',
        details: errorMsg
      })
    }
  },
  
  setMode: async (mode: BotMode) => {
    try {
      await botService.setMode(mode)
      set({ mode, error: null })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'info',
        message: `Bot mode changed to ${mode}`
      })
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Failed to change mode'
      set({ error: errorMsg })
      get().addLog({
        timestamp: new Date().toISOString(),
        level: 'error',
        message: 'Failed to change bot mode',
        details: errorMsg
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
  
  updateStatus: (status) => set({ status }),
  updateStats: (stats) => set({ stats }),
  setError: (error) => set({ error }),
}))
