// Vite environment variables
const getEnv = (key: string, fallback: string): string => {
  try {
    return (import.meta as any).env[key] || fallback
  } catch {
    return fallback
  }
}

export const API_BASE_URL = getEnv('VITE_API_BASE_URL', 'http://localhost:8080/api')
export const WS_URL = getEnv('VITE_WS_URL', 'ws://localhost:8080/ws')
export const APP_NAME = 'Sol Beast'
export const APP_VERSION = '1.0.0'

// Feature flags
export const FEATURES = {
  WEBSOCKET: false, // Enable when WS is fully implemented
  ADVANCED_CHARTS: true,
  EXPORT_REPORTS: false, // Enable in future
  MULTI_WALLET: false, // Enable in future
}

// UI Configuration
export const UI_CONFIG = {
  STATS_POLL_INTERVAL_MS: 2000,
  CHART_HISTORY_POINTS: 24,
  TOAST_DURATION_MS: 3000,
  DEBOUNCE_DELAY_MS: 300,
}

// Validation rules
export const VALIDATION = {
  MIN_BUY_AMOUNT: 0.0001,
  MAX_BUY_AMOUNT: 100,
  MIN_TP_PERCENT: 1,
  MAX_TP_PERCENT: 1000,
  MIN_SL_PERCENT: -99,
  MAX_SL_PERCENT: -1,
  MIN_HOLDINGS: 1,
  MAX_HOLDINGS: 100,
}
