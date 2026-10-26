import type {
  Dashboard,
  PoolsResponse,
  RentReport,
  RentScan,
  Status,
  VenuesResponse,
} from './types'

export const TOKEN_KEY = 'beast.session.token'

function envValue(key: string, fallback: string): string {
  const env = (import.meta as unknown as { env?: Record<string, string> }).env
  return env?.[key] || fallback
}

export const API_BASE = envValue('VITE_API_BASE_URL', '').replace(/\/$/, '')

export class ApiError extends Error {
  status: number

  constructor(message: string, status: number) {
    super(message)
    this.name = 'ApiError'
    this.status = status
  }
}

type RequestOptions = {
  method?: string
  body?: unknown
  token?: string | null
  signal?: AbortSignal
}

async function request<T>(path: string, options: RequestOptions = {}): Promise<T> {
  const headers: Record<string, string> = { Accept: 'application/json' }
  if (options.token) {
    headers.Authorization = `Bearer ${options.token}`
  }
  if (options.body !== undefined) {
    headers['Content-Type'] = 'application/json'
  }
  const controller = new AbortController()
  const timer = window.setTimeout(() => controller.abort(), 15_000)
  const abort = () => controller.abort()
  options.signal?.addEventListener('abort', abort, { once: true })
  try {
    const response = await fetch(`${API_BASE}${path}`, {
      method: options.method || 'GET',
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      signal: controller.signal,
    })
    const text = await response.text()
    let payload: unknown = null
    try {
      payload = text ? JSON.parse(text) : null
    } catch {
      payload = { error: text.slice(0, 300) || response.statusText }
    }
    if (!response.ok) {
      const detail =
        payload && typeof payload === 'object' && 'error' in payload
          ? String((payload as { error: unknown }).error)
          : response.statusText
      throw new ApiError(detail || `request failed (${response.status})`, response.status)
    }
    return payload as T
  } catch (error) {
    if (error instanceof ApiError) throw error
    if (error instanceof DOMException && error.name === 'AbortError') {
      throw new ApiError('request timed out', 0)
    }
    throw new ApiError(error instanceof Error ? error.message : 'network error', 0)
  } finally {
    window.clearTimeout(timer)
    options.signal?.removeEventListener('abort', abort)
  }
}

export function tokenFromStorage(): string {
  try {
    return sessionStorage.getItem(TOKEN_KEY) || envValue('VITE_BEAST_TOKEN', '')
  } catch {
    return envValue('VITE_BEAST_TOKEN', '')
  }
}

export function storeToken(token: string): void {
  try {
    if (token) sessionStorage.setItem(TOKEN_KEY, token)
    else sessionStorage.removeItem(TOKEN_KEY)
  } catch {
    // sessionStorage can be unavailable in hardened browser contexts.
  }
}

export const api = {
  health: () => request<{ status: string; version: string; uptime_seconds: number }>('/healthz'),
  status: (token: string, signal?: AbortSignal) =>
    request<Status>('/v1/status', { token, signal }),
  dashboard: (token: string, signal?: AbortSignal, limit = 100) =>
    request<Dashboard>(`/v1/dashboard?limit=${limit}`, { token, signal }),
  venues: (token: string, signal?: AbortSignal) =>
    request<VenuesResponse>('/v1/venues', { token, signal }),
  pools: (token: string, signal?: AbortSignal, source = 'feed') =>
    request<PoolsResponse>(`/v1/pools?limit=500&source=${source}`, { token, signal }),
  config: (token: string) => request<Record<string, unknown>>('/v1/config', { token }),
  report: (token: string) => request<Record<string, unknown>>('/v1/report', { token }),
  rent: (token: string, signal?: AbortSignal) =>
    request<RentScan>('/v1/rent', { token, signal }),
  reclaimRent: (token: string, confirm: boolean) =>
    request<RentReport>('/v1/rent/reclaim', {
      method: 'POST',
      token,
      body: { confirm },
    }),
  setMode: (token: string, mode: string) =>
    request<{ mode: string }>('/v1/mode', { method: 'POST', token, body: { mode } }),
  setStrategy: (token: string, name: string, mode: string) =>
    request<{ strategy: string; mode: string }>(`/v1/strategies/${encodeURIComponent(name)}`, {
      method: 'POST',
      token,
      body: { mode },
    }),
  kill: (token: string, reason: string) =>
    request<{ killed: boolean; reason: string }>('/v1/kill', {
      method: 'POST',
      token,
      body: { reason },
    }),
  resume: (token: string) => request<{ killed: boolean }>('/v1/resume', { method: 'POST', token }),
  refreshFeed: (token: string) =>
    request<{ attempted: number; updated: number; failed: number; duration_ms: number }>(
      '/v1/feed/refresh',
      { method: 'POST', token },
    ),
  addPool: (
    token: string,
    body: { address: string; venue: string; decimals_a?: number; decimals_b?: number; fee_bps?: number },
  ) => request<{ added: boolean }>('/v1/pools', { method: 'POST', token, body }),
  removePool: (token: string, address: string) =>
    request<{ removed: boolean }>(`/v1/pools/${encodeURIComponent(address)}`, {
      method: 'DELETE',
      token,
    }),
  quote: (token: string, inputMint: string, outputMint: string, amount: number, slippageBps = 50) =>
    request<Record<string, unknown>>('/v1/quote', {
      method: 'POST',
      token,
      body: {
        input_mint: inputMint,
        output_mint: outputMint,
        amount,
        slippage_bps: slippageBps,
      },
    }),
}
