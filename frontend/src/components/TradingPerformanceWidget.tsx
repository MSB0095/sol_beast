import { TrendingUp, TrendingDown } from 'lucide-react'

interface Trade {
  token: string
  symbol: string
  profit: number
  change: number
  volume: string
  iconColor: string
}

export default function TradingPerformanceWidget() {
  // Mock data - this would come from your bot store in production
  const topTrades: Trade[] = [
    {
      token: 'SOL',
      symbol: 'Solana',
      profit: 2.456,
      change: 15.3,
      volume: '1.2k',
      iconColor: '#9945FF'
    },
    {
      token: 'BONK',
      symbol: 'Bonk',
      profit: 0.892,
      change: 8.7,
      volume: '890',
      iconColor: '#FF6B00'
    },
    {
      token: 'JUP',
      symbol: 'Jupiter',
      profit: -0.234,
      change: -3.2,
      volume: '450',
      iconColor: '#00D4AA'
    },
    {
      token: 'WIF',
      symbol: 'Dogwifhat',
      profit: 1.567,
      change: 12.1,
      volume: '780',
      iconColor: '#FF9500'
    },
    {
      token: 'PYTH',
      symbol: 'Pyth Network',
      profit: -0.123,
      change: -1.8,
      volume: '320',
      iconColor: '#6C5CE7'
    },
  ]

  return (
    <div className="cyber-card p-6 animate-fade-in-up">
      <div className="flex items-start justify-between gap-2 mb-6">
        <div>
          <h4 className="font-display text-xl font-black glow-text uppercase tracking-wider">
            Top Performing Tokens
          </h4>
          <span className="text-[var(--theme-text-secondary)] text-sm font-mono-tech uppercase tracking-widest mt-1 inline-block">
            Real-time Performance
          </span>
        </div>
        <div className="relative inline-flex">
          <button
            type="button"
            className="btn btn-sm px-3 py-1 font-mono-tech text-xs uppercase tracking-wider"
            style={{
              background: 'var(--theme-bg-secondary)',
              border: '1px solid var(--theme-accent)',
              color: 'var(--theme-accent)',
              boxShadow: '0 0 10px var(--glow-color)'
            }}
          >
            <span className="icon-[tabler--refresh] size-4"></span>
            Refresh
          </button>
        </div>
      </div>

      <div className="space-y-5">
        {topTrades.map((trade, index) => (
          <div key={index} className="group">
            <div className="flex items-center gap-3 p-3 rounded-lg transition-all duration-300 hover:bg-[var(--glass-bg)]">
              {/* Token Icon */}
              <div 
                className="w-11 h-11 rounded-full flex items-center justify-center font-bold text-white relative overflow-hidden group-hover:scale-110 transition-transform"
                style={{
                  background: `linear-gradient(135deg, ${trade.iconColor}, ${trade.iconColor}99)`,
                  boxShadow: `0 0 20px ${trade.iconColor}66`
                }}
              >
                <span className="text-sm font-display font-black">{trade.token}</span>
                {/* Animated ring */}
                <div 
                  className="absolute inset-0 rounded-full animate-pulse"
                  style={{
                    border: `2px solid ${trade.iconColor}`,
                    opacity: 0.3
                  }}
                />
              </div>

              {/* Token Info */}
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2.5">
                  <h6 className="font-display font-bold text-white">
                    ◎{Math.abs(trade.profit).toFixed(3)}
                  </h6>
                  <div className={`flex items-center ${trade.change >= 0 ? 'text-[var(--theme-success)]' : 'text-[var(--theme-error)]'}`}>
                    {trade.change >= 0 ? (
                      <TrendingUp size={14} />
                    ) : (
                      <TrendingDown size={14} />
                    )}
                    <p className="text-sm font-mono-tech ml-1">{Math.abs(trade.change).toFixed(1)}%</p>
                  </div>
                </div>
                <p className="text-[var(--theme-text-secondary)] text-sm font-mono-tech mt-0.5">
                  {trade.symbol}
                </p>
              </div>

              {/* Volume */}
              <span 
                className="font-mono-tech font-semibold text-sm px-3 py-1 rounded-full"
                style={{
                  background: 'var(--glass-bg)',
                  border: '1px solid var(--theme-accent)',
                  color: 'var(--theme-accent)',
                  boxShadow: '0 0 10px var(--glow-color)'
                }}
              >
                {trade.volume}
              </span>
            </div>

            {/* Separator with glow effect */}
            {index < topTrades.length - 1 && (
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
        ))}
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
