import { useState, useEffect } from 'react'
import { Coins, ExternalLink, TrendingUp, Clock, User, DollarSign } from 'lucide-react'
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

export default function NewCoinsPanel() {
  const [coins, setCoins] = useState<DetectedCoin[]>([])
  const [filter, setFilter] = useState<'all' | 'detected' | 'bought' | 'skipped'>('all')

  useEffect(() => {
    const fetchCoins = async () => {
      try {
        const response = await fetch(API_DETECTED_COINS_URL)
        if (response.ok) {
          const data = await response.json()
          console.debug('Detected coins response:', data)
          // API returns { coins: [...], total: N }
          if (data && Array.isArray(data.coins)) {
            setCoins(data.coins)
          } else if (Array.isArray(data)) {
            setCoins(data)
          }
        }
      } catch (error) {
        console.error('Failed to fetch detected coins:', error)
      }
    }
    
    // Fetch immediately
    fetchCoins()
    
    // Poll every 2 seconds
    const interval = setInterval(fetchCoins, 2000)
    return () => clearInterval(interval)
  }, [])

  const filteredCoins = filter === 'all' 
    ? coins 
    : coins.filter(coin => coin.status === filter)

  if (coins.length === 0) {
    return (
      <div className="card-enhanced rounded-xl p-12 text-center">
        <Coins size={48} className="mx-auto text-gray-500 mb-4 opacity-50" />
        <p className="text-gray-400">No new coins detected yet</p>
        <p className="text-gray-500 text-sm">Bot is monitoring for new token launches</p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Filter Tabs */}
      <div className="card-enhanced rounded-xl p-4">
        <div className="flex gap-2">
          {(['all', 'detected', 'bought', 'skipped'] as const).map((status) => (
            <button
              key={status}
              onClick={() => setFilter(status)}
              className="px-4 py-2 rounded-lg transition-all uppercase tracking-wider font-semibold"
              style={filter === status ? {
                backgroundColor: 'var(--theme-button-bg)',
                color: 'var(--theme-button-text)',
                border: '2px solid var(--theme-accent)',
                boxShadow: '0 0 15px var(--glow-color)'
              } : {
                backgroundColor: 'var(--theme-bg-secondary)',
                color: 'var(--theme-text-muted)',
                border: '2px solid transparent'
              }}
            >
              {status} {status === 'all' ? `(${coins.length})` : `(${coins.filter(c => c.status === status).length})`}
            </button>
          ))}
        </div>
      </div>

      {/* Coins Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {filteredCoins.map((coin) => (
          <div 
            key={coin.mint} 
            className="cyber-card rounded-xl p-6 hover:border-[var(--theme-accent)] transition-all stagger-item"
            style={{ animationDelay: `${filteredCoins.indexOf(coin) * 0.05}s` }}
          >
            <div className="flex gap-4">
              {/* Token Image */}
              <div className="flex-shrink-0">
                {coin.image ? (
                  <img 
                    src={coin.image} 
                    alt={coin.name || coin.symbol || 'Token'} 
                    className="w-16 h-16 rounded-lg object-cover"
                    onError={(e) => {
                      e.currentTarget.src = 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="%23374151"/><text x="32" y="32" font-size="24" text-anchor="middle" dy=".3em" fill="%239CA3AF">?</text></svg>'
                    }}
                  />
                ) : (
                  <div className="w-16 h-16 rounded-lg bg-gray-700 flex items-center justify-center">
                    <Coins size={32} className="text-gray-500" />
                  </div>
                )}
              </div>

              {/* Token Info */}
              <div className="flex-1 min-w-0">
                <div className="flex items-start justify-between mb-2">
                  <div>
                    <h3 className="text-lg font-semibold truncate">
                      {coin.name || coin.symbol || 'Unknown Token'}
                    </h3>
                    {coin.symbol && coin.name && (
                      <p className="text-sm text-gray-400">${coin.symbol}</p>
                    )}
                  </div>
                  <span className={`px-2 py-1 rounded text-xs font-semibold ${
                    coin.status === 'bought' 
                      ? 'bg-green-900/30 text-green-400' 
                      : coin.status === 'detected'
                      ? 'bg-blue-900/30 text-blue-400'
                      : 'bg-gray-700 text-gray-400'
                  }`}>
                    {coin.status}
                  </span>
                </div>

                {/* Details */}
                <div className="space-y-2 text-sm">
                  <div className="flex items-center gap-2 text-gray-400">
                    <Clock size={14} />
                    <span className="text-xs">
                      {new Date(coin.detected_at).toLocaleString()}
                    </span>
                  </div>

                  <div className="flex items-center gap-2">
                    <User size={14} className="text-gray-400" />
                    <span className="text-xs font-mono text-gray-400 truncate">
                      Creator: {coin.creator.slice(0, 8)}...{coin.creator.slice(-8)}
                    </span>
                    <a
                      href={`https://solscan.io/account/${coin.creator}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sol-purple hover:text-sol-purple-light transition-colors"
                    >
                      <ExternalLink size={14} />
                    </a>
                  </div>

                  <div className="flex items-center gap-2">
                    <TrendingUp size={14} className="text-gray-400" />
                    <span className="text-xs font-mono text-gray-400 truncate">
                      Curve: {coin.bonding_curve.slice(0, 8)}...{coin.bonding_curve.slice(-8)}
                    </span>
                    <a
                      href={`https://solscan.io/account/${coin.bonding_curve}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sol-purple hover:text-sol-purple-light transition-colors"
                    >
                      <ExternalLink size={14} />
                    </a>
                  </div>

                  {/* Buy Price */}
                  <div className="flex items-center gap-2">
                    <DollarSign size={14} className="text-gray-400" />
                    <span className="text-xs font-mono text-gray-400 truncate">
                      Buy Price: {coin.buy_price != null && coin.buy_price !== undefined ? `${coin.buy_price.toFixed(6)} SOL` : 'N/A'}
                    </span>
                  </div>

                  {coin.buy_price != null && coin.buy_price !== undefined && (
                    <div className="flex items-center gap-2 pt-2 border-t border-gray-700">
                      <span className="text-xs text-gray-400">Buy Price:</span>
                      <span className="text-sm font-semibold text-sol-purple">
                        {coin.buy_price.toFixed(9)} SOL/token
                      </span>
                    </div>
                  )}
                </div>

                {/* Mint Address (Bottom) */}
                <div className="mt-3 pt-3 border-t border-gray-700">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-mono text-gray-500 truncate">
                      {coin.mint}
                    </span>
                    <div className="flex gap-2">
                      <a
                        href={`https://solscan.io/token/${coin.mint}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-sol-purple hover:text-sol-purple-light transition-colors"
                        title="View on Solscan"
                      >
                        <ExternalLink size={16} />
                      </a>
                      <button
                        onClick={() => navigator.clipboard.writeText(coin.mint)}
                        className="text-gray-400 hover:text-white transition-colors"
                        title="Copy mint address"
                      >
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                          <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                        </svg>
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
