import { useState, useEffect } from 'react'
import { TrendingUp, TrendingDown } from 'lucide-react'
import { API_TRADES_URL } from '../config'

interface Trade {
  mint: string
  symbol?: string
  name?: string
  image?: string
  trade_type: string
  timestamp: string
  amount_sol: number
  price_per_token: number
  profit_loss?: number
  profit_loss_percent?: number
  reason?: string
}

export default function TradingPerformanceWidget() {
  const [trades, setTrades] = useState<Trade[]>([])

  useEffect(() => {
    const fetchTrades = async () => {
      try {
        const response = await fetch(API_TRADES_URL)
        if (response.ok) {
          const data: Trade[] = await response.json()
          setTrades(data)
        }
      } catch (error) {
        console.error('Failed to fetch trades:', error)
      }
    }
    fetchTrades()
    const interval = setInterval(fetchTrades, 3000)
    return () => clearInterval(interval)
  }, [])

  // Aggregate PnL per token from sell trades
  const tokenPnL = new Map<string, { symbol: string; name: string; image?: string; profit: number; pctSum: number; count: number }>()
  for (const t of trades) {
    if (t.trade_type === 'sell' && t.profit_loss != null) {
      const key = t.mint
      const existing = tokenPnL.get(key)
      if (existing) {
        existing.profit += t.profit_loss
        existing.pctSum += t.profit_loss_percent || 0
        existing.count += 1
      } else {
        tokenPnL.set(key, {
          symbol: t.symbol || t.mint.slice(0, 6),
          name: t.name || t.symbol || 'Unknown',
          image: t.image,
          profit: t.profit_loss,
          pctSum: t.profit_loss_percent || 0,
          count: 1,
        })
      }
    }
  }

  // Sort by profit descending
  const topTokens = Array.from(tokenPnL.entries())
    .map(([mint, data]) => ({ mint, ...data, avgPct: data.pctSum / data.count }))
    .sort((a, b) => b.profit - a.profit)
    .slice(0, 5)

  const totalProfit = topTokens.reduce((sum, t) => sum + t.profit, 0)
  const totalSells = trades.filter(t => t.trade_type === 'sell').length
  const totalBuys = trades.filter(t => t.trade_type === 'buy').length

  if (trades.length === 0) {
    return (
      <div className="cyber-card p-6 animate-fade-in-up">
        <h4 className="font-display text-xl font-black glow-text uppercase tracking-wider mb-4">
          Top Performing Tokens
        </h4>
        <p className="text-[var(--theme-text-secondary)] text-sm font-mono-tech">
          No trades yet — performance data will appear after sells.
        </p>
      </div>
    )
  }

  return (
    <div className="cyber-card p-6 animate-fade-in-up">
      <div className="flex items-start justify-between gap-2 mb-6">
        <div>
          <h4 className="font-display text-xl font-black glow-text uppercase tracking-wider">
            Top Performing Tokens
          </h4>
          <span className="text-[var(--theme-text-secondary)] text-sm font-mono-tech uppercase tracking-widest mt-1 inline-block">
            {totalBuys} buys · {totalSells} sells · Net: {totalProfit >= 0 ? '+' : ''}{totalProfit.toFixed(4)} SOL
          </span>
        </div>
      </div>

      <div className="space-y-5">
        {topTokens.map((token, index) => {
          const isPositive = token.profit >= 0
          const color = isPositive ? 'var(--theme-success, #22c55e)' : 'var(--theme-error, #ef4444)'
          const rankClass = index === 0 ? 'rank-badge-gold' : index === 1 ? 'rank-badge-silver' : index === 2 ? 'rank-badge-bronze' : ''

          return (
            <div key={token.mint} className="group stagger-item" style={{ animationDelay: `${index * 0.1}s` }}>
              <div className={`flex items-center gap-3 p-3 rounded-lg transition-all duration-300 hover:bg-[var(--glass-bg)] ${isPositive ? 'profit-row-shimmer' : ''}`}>
                {/* Rank Badge */}
                <div className={`rank-badge ${rankClass}`}>
                  {index + 1}
                </div>
                <div 
                  className="w-11 h-11 rounded-full flex items-center justify-center font-bold text-white relative overflow-hidden group-hover:scale-110 transition-transform"
                  style={{
                    background: token.image ? `url(${token.image}) center/cover` : `linear-gradient(135deg, ${color}, ${color}99)`,
                    boxShadow: `0 0 20px ${color}66`
                  }}
                >
                  {!token.image && <span className="text-xs font-display font-black">{token.symbol.slice(0, 3)}</span>}
                </div>

                {/* Token Info */}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2.5">
                    <h6 className="font-display font-bold text-white">
                      ◎{Math.abs(token.profit).toFixed(4)}
                    </h6>
                    <div className="flex items-center" style={{ color }}>
                      {isPositive ? <TrendingUp size={14} /> : <TrendingDown size={14} />}
                      <p className="text-sm font-mono-tech ml-1">{isPositive ? '+' : ''}{token.avgPct.toFixed(1)}%</p>
                    </div>
                  </div>
                  <p className="text-[var(--theme-text-secondary)] text-sm font-mono-tech mt-0.5">
                    {token.name} ({token.symbol})
                  </p>
                </div>

                {/* Trade count */}
                <span 
                  className="font-mono-tech font-semibold text-sm px-3 py-1 rounded-full"
                  style={{
                    background: 'var(--glass-bg)',
                    border: '1px solid var(--theme-accent)',
                    color: 'var(--theme-accent)',
                    boxShadow: '0 0 10px var(--glow-color)'
                  }}
                >
                  {token.count} sell{token.count > 1 ? 's' : ''}
                </span>
              </div>

              {/* Separator */}
              {index < topTokens.length - 1 && (
                <div 
                  className="h-[1px] mt-5 relative"
                  style={{
                    background: 'linear-gradient(90deg, transparent, var(--theme-accent)44, transparent)'
                  }}
                >
                  <div 
                    className="absolute top-1/2 left-1/2 w-2 h-2 rounded-full -translate-x-1/2 -translate-y-1/2 animate-pulse"
                    style={{
                      backgroundColor: 'var(--theme-accent)',
                      boxShadow: '0 0 8px var(--glow-color-strong)'
                    }}
                  />
                </div>
              )}
            </div>
          )
        })}
      </div>

      {/* View All Link */}
      <div className="mt-6 pt-5 border-t border-[var(--border-glow)]">
        <button 
          className="w-full py-3 font-mono-tech text-sm uppercase tracking-widest transition-all duration-300 rounded-lg"
          style={{
            background: 'var(--theme-bg-secondary)',
            border: '2px solid var(--theme-accent)',
            color: 'var(--theme-accent)',
            boxShadow: '0 0 15px var(--glow-color)'
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = 'var(--theme-accent)'
            e.currentTarget.style.color = '#000000'
            e.currentTarget.style.boxShadow = '0 0 30px var(--glow-color-strong)'
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = 'var(--theme-bg-secondary)'
            e.currentTarget.style.color = 'var(--theme-accent)'
            e.currentTarget.style.boxShadow = '0 0 15px var(--glow-color)'
          }}
        >
          <span className="icon-[tabler--chart-bar] inline-block w-4 h-4 mr-2"></span>
          View All Trades
        </button>
      </div>
    </div>
  )
}
