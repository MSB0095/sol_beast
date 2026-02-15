// Vite environment variables
const getEnv = (key: string, fallback: string): string => {
  try {
    return (import.meta as any).env[key] || fallback
  } catch {
    return fallback
  }
}

// Use relative URLs so the Vite dev server proxy (or production reverse proxy)
// forwards requests to the backend. This avoids hardcoding localhost which
// breaks in environments like GitHub Codespaces.
export const API_BASE_URL = getEnv('VITE_API_BASE_URL', '/api')
export const API_HEALTH_URL = `${API_BASE_URL}/health`
export const API_STATS_URL = `${API_BASE_URL}/stats`
export const API_BOT_STATE_URL = `${API_BASE_URL}/bot/state`
export const API_BOT_MODE_URL = `${API_BASE_URL}/bot/mode`
export const API_BOT_START_URL = `${API_BASE_URL}/bot/start`
export const API_BOT_STOP_URL = `${API_BASE_URL}/bot/stop`
export const API_LOGS_URL = `${API_BASE_URL}/logs`
export const API_DETECTED_COINS_URL = `${API_BASE_URL}/detected-coins`
export const API_TRADES_URL = `${API_BASE_URL}/trades`
export const API_SETTINGS_URL = `${API_BASE_URL}/settings`
// WebSocket URL derived dynamically from current page location
const wsProtocol = typeof window !== 'undefined' && window.location.protocol === 'https:' ? 'wss:' : 'ws:'
const wsHost = typeof window !== 'undefined' ? window.location.host : 'localhost:3000'
export const WS_URL = getEnv('VITE_WS_URL', `${wsProtocol}//${wsHost}/api/ws`)
export const APP_NAME = 'Sol Beast'
export const APP_VERSION = '1.0.0'

// Feature flags
export const FEATURES = {
  WEBSOCKET: true, // Enable WebSocket functionality
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
  MAX_LOGS: 100,
  CONNECTION_RETRY_DELAY_MS: 3000,
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
