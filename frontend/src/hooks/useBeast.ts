import { useCallback, useEffect, useRef, useState } from 'react'
import { api, ApiError } from '../api'
import type { Dashboard, PoolsResponse, VenuesResponse } from '../types'

export interface HistoryPoint {
  t: string
  opportunities: number
  modeled: number
  realized: number
  bundles: number
  landed: number
}

export function useDashboard(
  token: string,
  onUnauthorized: () => void,
  intervalMs = 2_500,
) {
  const [data, setData] = useState<Dashboard | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)
  const [lastUpdated, setLastUpdated] = useState<number | null>(null)
  const [history, setHistory] = useState<HistoryPoint[]>([])
  const inFlight = useRef(false)
  const abortRef = useRef<AbortController | null>(null)

  const refresh = useCallback(
    async (quiet = false) => {
      if (inFlight.current) return
      inFlight.current = true
      if (!quiet) setLoading(true)
      const controller = new AbortController()
      abortRef.current = controller
      try {
        const next = await api.dashboard(token, controller.signal, 120)
        setData(next)
        setLastUpdated(Date.now())
        setError(null)
        const point: HistoryPoint = {
          t: new Date().toLocaleTimeString([], {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
            hour12: false,
          }),
          opportunities: next.status.metrics.opportunities,
          modeled: next.opportunities.reduce((sum, item) => sum + item.est_net_lamports, 0),
          realized: next.status.metrics.realized_net_lamports,
          bundles: next.status.metrics.bundles_submitted,
          landed: next.status.metrics.bundles_landed,
        }
        setHistory((current) => [...current.slice(-47), point])
      } catch (caught) {
        if (caught instanceof ApiError && caught.status === 401) {
          onUnauthorized()
          return
        }
        setError(caught instanceof Error ? caught.message : 'unable to reach the engine')
      } finally {
        inFlight.current = false
        if (!quiet) setLoading(false)
      }
    },
    [onUnauthorized, token],
  )

  useEffect(() => {
    void refresh()
    const id = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh(true)
    }, intervalMs)
    return () => {
      window.clearInterval(id)
      abortRef.current?.abort()
    }
  }, [refresh, intervalMs])

  return { data, error, loading, lastUpdated, history, refresh }
}

export function useVenues(token: string, onUnauthorized: () => void) {
  const [data, setData] = useState<VenuesResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    setLoading(true)
    const controller = new AbortController()
    try {
      const next = await api.venues(token, controller.signal)
      setData(next)
      setError(null)
    } catch (caught) {
      if (caught instanceof ApiError && caught.status === 401) {
        onUnauthorized()
        return
      }
      setError(caught instanceof Error ? caught.message : 'unable to load venues')
    } finally {
      setLoading(false)
    }
  }, [onUnauthorized, token])

  useEffect(() => {
    void refresh()
  }, [refresh])

  return { data, error, loading, refresh }
}

export function usePools(token: string, onUnauthorized: () => void) {
  const [data, setData] = useState<PoolsResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [loading, setLoading] = useState(true)

  const refresh = useCallback(async () => {
    setLoading(true)
    try {
      const next = await api.pools(token, undefined, 'feed')
      setData(next)
      setError(null)
    } catch (caught) {
      if (caught instanceof ApiError && caught.status === 401) {
        onUnauthorized()
        return
      }
      setError(caught instanceof Error ? caught.message : 'unable to load pools')
    } finally {
      setLoading(false)
    }
  }, [onUnauthorized, token])

  useEffect(() => {
    void refresh()
    const id = window.setInterval(() => {
      if (document.visibilityState === 'visible') void refresh()
    }, 5_000)
    return () => window.clearInterval(id)
  }, [refresh])

  return { data, error, loading, refresh }
}
