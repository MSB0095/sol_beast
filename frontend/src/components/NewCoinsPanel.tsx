import { useState, useEffect, useCallback } from 'react'
import { Coins, ExternalLink, TrendingUp, Clock, User, Plus } from 'lucide-react'
import { API_DETECTED_COINS_URL } from '../config'

interface DetectedCoin {
  mint: string
  name?: string
  symbol?: string
  image?: string
  creator: string
  bonding_curve: string
  detected_at: string
  metadata_uri?: string
  buy_price?: number
  status: 'detected' | 'bought' | 'skipped'
}

const extractDetectedCoins = (data: unknown): DetectedCoin[] | null => {
  if (Array.isArray(data)) {
    return data.filter((d): d is DetectedCoin => typeof d === 'object' && d !== null && 'mint' in d && 'detected_at' in d)
  }
  if (data && typeof data === 'object') {
    const obj = data as Record<string, unknown>
    for (const key of ['coins', 'detected', 'items', 'data']) {
      const candidate = obj[key]
      if (Array.isArray(candidate)) {
        return candidate.filter((d): d is DetectedCoin => typeof d === 'object' && d !== null && 'mint' in d && 'detected_at' in d)
      }
    }
    // try to find any array value
    const maybeArray = Object.values(obj).find(v => Array.isArray(v))
    if (Array.isArray(maybeArray)) {
      return (maybeArray as unknown[]).filter((d): d is DetectedCoin => typeof d === 'object' && d !== null && 'mint' in d && 'detected_at' in d)
    }
  }
  return null
}

export default function NewCoinsPanel() {
  const [coins, setCoins] = useState<DetectedCoin[]>([])
  const [filter, setFilter] = useState<'all' | 'detected' | 'bought' | 'skipped'>('all')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const fetchCoins = useCallback(async () => {
    setLoading(true)
    setError(null)
    try {
      const response = await fetch(API_DETECTED_COINS_URL)
      if (!response.ok) {
        const text = await response.text().catch(() => '')
        setError(`Failed to fetch detected coins: ${response.status}`)
        console.debug('Detected coins error response body:', text)
        setLoading(false)
        return
      }

      const data = await response.json().catch((e) => {
        console.debug('Failed to parse detected coins JSON', e)
        return null
      })

      const extracted = extractDetectedCoins(data)
      if (extracted && extracted.length > 0) {
        setCoins(extracted)
        setError(null)
      } else {
        setCoins([])
        setError(data ? 'No detected coins found in response' : 'Invalid response')
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error('Failed to fetch detected coins:', msg)
      setError(msg)
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    // Fetch immediately
    fetchCoins()

    // Poll every 2 seconds
    const interval = setInterval(fetchCoins, 2000)
    return () => clearInterval(interval)
  }, [fetchCoins])

  const filteredCoins = filter === 'all'
    ? coins
    : coins.filter(coin => coin.status === filter)

  if (coins.length === 0) {
    return (
      <div className="card bg-base-200/50 border border-base-300 rounded-xl p-6 text-center">
        {loading ? (
          <div className="py-12">Loading detected coins...</div>
        ) : error ? (
          <div className="py-6">
            <div className="alert alert-warning mb-4">
              <div>
                <strong>Detected coins:</strong> {error}
              </div>
            </div>
            <div className="flex items-center justify-center gap-3">
              <button onClick={() => fetchCoins()} className="btn btn-sm">Retry</button>
            </div>
          </div>
        ) : (
          <div className="flex flex-col items-center gap-4 py-12">
            <div className="p-4 bg-base-100 rounded-full">
              <Coins className="w-12 h-12 text-base-content/50" />
            </div>
            <div>
              <h3 className="text-lg font-semibold text-base-content mb-2">No New Coins Detected</h3>
              <p className="text-base-content/60">Bot is monitoring for new token launches</p>
            </div>
          </div>
        )}
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="p-3 bg-primary/10 rounded-lg">
          <Plus className="w-6 h-6 text-primary" />
        </div>
        <div>
          <h3 className="text-xl font-bold text-base-content uppercase tracking-wider">
            New Coins Detected
          </h3>
          <p className="text-base-content/60">
            {coins.length} coin{coins.length !== 1 ? 's' : ''} discovered
          </p>
        </div>
      </div>

      {/* Filter Tabs */}
      <div className="card bg-base-200/50 border border-base-300 rounded-xl">
        <div className="card-body">
          <div className="flex flex-wrap gap-2">
            {(['all', 'detected', 'bought', 'skipped'] as const).map((status) => (
              <button
                key={status}
                onClick={() => setFilter(status)}
                className={`btn btn-sm capitalize ${
                  filter === status
                    ? 'btn-primary'
                    : 'btn-soft btn-ghost'
                }`}
              >
                {status}
                <span className="badge badge-sm ml-2">
                  {status === 'all' ? coins.length : coins.filter(c => c.status === status).length}
                </span>
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Coins Grid - Using flyonui Table-style cards */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {filteredCoins.map((coin) => (
          <div
            key={coin.mint}
            className="card bg-base-200/50 border border-base-300 rounded-xl hover:border-primary/50 transition-all duration-300 hover:shadow-lg"
          >
            <div className="card-body p-6">
              <div className="flex gap-4">
                {/* Token Image */}
                <div className="flex-shrink-0">
                  {coin.image ? (
                    <img
                      src={coin.image}
                      alt={coin.name || coin.symbol || 'Token'}
                      className="w-16 h-16 rounded-lg object-cover border border-base-300"
                      onError={(e) => {
                        e.currentTarget.style.display = 'none'
                      }}
                    />
                  ) : (
                    <div className="w-16 h-16 rounded-lg bg-base-300 flex items-center justify-center">
                      <Coins className="w-8 h-8 text-base-content/50" />
                    </div>
                  )}
                </div>

                {/* Token Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex-1 min-w-0">
                      <h3 className="text-lg font-semibold text-base-content truncate">
                        {coin.name || coin.symbol || 'Unknown Token'}
                      </h3>
                      {coin.symbol && coin.name && (
                        <p className="text-sm text-base-content/60">${coin.symbol}</p>
                      )}
                    </div>
                    <span className={`badge badge-soft ${
                      coin.status === 'bought'
                        ? 'badge-success'
                        : coin.status === 'detected'
                        ? 'badge-info'
                        : 'badge-ghost'
                    }`}>
                      {coin.status}
                    </span>
                  </div>

                  {/* Details Grid */}
                  <div className="grid grid-cols-1 gap-2 text-sm mb-4">
                    <div className="flex items-center gap-2 text-base-content/60">
                      <Clock className="w-4 h-4" />
                      <span className="text-xs">
                        {new Date(coin.detected_at).toLocaleDateString()} {new Date(coin.detected_at).toLocaleTimeString()}
                      </span>
                    </div>

                    <div className="flex items-center gap-2">
                      <User className="w-4 h-4 text-base-content/60" />
                      <span className="text-xs font-mono text-base-content/60 truncate flex-1">
                        {coin.creator.slice(0, 8)}...{coin.creator.slice(-8)}
                      </span>
                      <a
                        href={`https://solscan.io/account/${coin.creator}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="btn btn-circle btn-text btn-xs"
                        title="View creator on Solscan"
                      >
                        <ExternalLink className="w-3 h-3" />
                      </a>
                    </div>

                    <div className="flex items-center gap-2">
                      <TrendingUp className="w-4 h-4 text-base-content/60" />
                      <span className="text-xs font-mono text-base-content/60 truncate flex-1">
                        {coin.bonding_curve.slice(0, 8)}...{coin.bonding_curve.slice(-8)}
                      </span>
                      <a
                        href={`https://solscan.io/account/${coin.bonding_curve}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="btn btn-circle btn-text btn-xs"
                        title="View bonding curve on Solscan"
                      >
                        <ExternalLink className="w-3 h-3" />
                      </a>
                    </div>

                    {coin.buy_price && (
                      <div className="flex items-center gap-2 pt-2 border-t border-base-300">
                        <span className="text-xs text-base-content/60">Buy Price:</span>
                        <span className="text-sm font-semibold text-primary">
                          {coin.buy_price.toFixed(9)} SOL/token
                        </span>
                      </div>
                    )}
                  </div>

                  {/* Mint Address */}
                  <div className="bg-base-100 rounded-lg p-3">
                    <div className="flex items-center justify-between gap-2">
                      <div className="flex-1 min-w-0">
                        <span className="text-xs font-mono text-base-content/60 break-all">
                          {coin.mint}
                        </span>
                      </div>
                      <div className="flex gap-1">
                        <button
                          onClick={() => navigator.clipboard.writeText(coin.mint)}
                          className="btn btn-circle btn-text btn-xs"
                          title="Copy mint address"
                        >
                          <svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                            <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                          </svg>
                        </button>
                        <a
                          href={`https://solscan.io/token/${coin.mint}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="btn btn-circle btn-text btn-xs"
                          title="View on Solscan"
                        >
                          <ExternalLink className="w-3 h-3" />
                        </a>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>

      {/* Summary Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="card bg-info/10 border border-info/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <Plus className="w-5 h-5 text-info" />
            <span className="text-sm font-medium text-info/80 uppercase">Total Detected</span>
          </div>
          <p className="text-2xl font-bold text-info">{coins.length}</p>
        </div>
        
        <div className="card bg-info/10 border border-info/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <Clock className="w-5 h-5 text-info" />
            <span className="text-sm font-medium text-info/80 uppercase">New Today</span>
          </div>
          <p className="text-2xl font-bold text-info">
            {coins.filter(c => new Date(c.detected_at).toDateString() === new Date().toDateString()).length}
          </p>
        </div>
        
        <div className="card bg-success/10 border border-success/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <TrendingUp className="w-5 h-5 text-success" />
            <span className="text-sm font-medium text-success/80 uppercase">Bought</span>
          </div>
          <p className="text-2xl font-bold text-success">
            {coins.filter(c => c.status === 'bought').length}
          </p>
        </div>
        
        <div className="card bg-ghost/10 border border-ghost/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <Coins className="w-5 h-5 text-base-content/60" />
            <span className="text-sm font-medium text-base-content/60 uppercase">Skipped</span>
          </div>
          <p className="text-2xl font-bold text-base-content/60">
            {coins.filter(c => c.status === 'skipped').length}
          </p>
        </div>
      </div>
    </div>
  )
}
