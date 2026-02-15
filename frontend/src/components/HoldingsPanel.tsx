import { useState, useEffect } from 'react'
import { useBotStore } from '../store/botStore'
import { Clock, TrendingUp, TrendingDown, Minus } from 'lucide-react'

function formatHoldTime(buyTimeStr: string) {
  const elapsed = Math.max(0, Math.floor((Date.now() - new Date(buyTimeStr).getTime()) / 1000))
  const m = Math.floor(elapsed / 60)
  const s = elapsed % 60
  return `${m}m ${s}s`
}

export default function HoldingsPanel() {
  const { stats, prices } = useBotStore()
  // Force re-render every second so hold times and PnL stay live
  const [, setTick] = useState(0)
  useEffect(() => {
    const id = setInterval(() => setTick(t => t + 1), 1000)
    return () => clearInterval(id)
  }, [])

  if (!stats?.current_holdings || stats.current_holdings.length === 0) {
    return (
      <div className="card-enhanced rounded-xl p-12 text-center">
        <Clock size={48} className="mx-auto text-gray-500 mb-4 opacity-50" />
        <p className="text-gray-400 font-semibold">No active holdings</p>
        <p className="text-gray-500 text-sm mt-2">Trades will appear here when they're active</p>
      </div>
    )
  }

  // Compute aggregate live PnL across all holdings — always derived from
  // holding.buy_price (authoritative, from stats API) and the latest WS price.
  // This avoids stale / zero PnL values that can arrive from the backend when
  // the WSS handler races with the monitor loop.
  let totalPnlSol = 0
  let totalPositionValueSol = 0
  let totalBuyCostSol = 0

  stats.current_holdings.forEach(h => {
    const live = prices[h.mint]
    const currentPrice = live ? live.price : h.buy_price
    const decimals = h.decimals || 6
    const tokenDivisor = Math.pow(10, decimals)
    const tokens = h.amount / tokenDivisor
    const positionValue = currentPrice * tokens
    const buyCost = h.buy_price * tokens
    totalPositionValueSol += positionValue
    totalBuyCostSol += buyCost
    totalPnlSol += positionValue - buyCost
  })

  const totalPnlPercent = totalBuyCostSol > 0 ? ((totalPositionValueSol - totalBuyCostSol) / totalBuyCostSol) * 100 : 0
  const totalPnlClass = totalPnlSol > 0.000000001 ? 'text-green-400' : totalPnlSol < -0.000000001 ? 'text-red-400' : 'text-gray-400'

  return (
    <div className="space-y-4">
      {/* Live Aggregate PnL Banner */}
      <div className={`card-enhanced rounded-xl p-4 ${totalPnlSol > 0.000000001 ? 'profit-glow' : ''}`}>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            {totalPnlSol > 0 ? <TrendingUp className="text-green-400 breathing-dot" size={24} /> :
             totalPnlSol < 0 ? <TrendingDown className="text-red-400" size={24} /> :
             <Minus className="text-gray-500" size={24} />}
            <div>
              <p className="text-xs text-gray-400 uppercase tracking-wider font-mono">Live Unrealized PnL</p>
              <p className={`text-2xl font-bold font-mono ${totalPnlClass}`}>
                {totalPnlSol >= 0 ? '+' : ''}{totalPnlSol.toFixed(9)} SOL
                <span className="text-sm ml-2">({totalPnlPercent >= 0 ? '+' : ''}{totalPnlPercent.toFixed(2)}%)</span>
              </p>
            </div>
          </div>
          <div className="text-right">
            <p className="text-xs text-gray-400 uppercase tracking-wider font-mono">Position Value</p>
            <p className="text-lg font-bold font-mono text-blue-400">
              {totalPositionValueSol.toFixed(9)} SOL
            </p>
          </div>
        </div>
      </div>

      <div className="card-enhanced rounded-xl p-6">
        <h3 className="text-lg font-semibold mb-4 gradient-text">Current Holdings ({stats.current_holdings.length})</h3>
        
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-gray-400 border-b border-gray-700">
                <th className="text-left py-3 px-4">Token</th>
                <th className="text-left py-3 px-4">Name/Symbol</th>
                <th className="text-right py-3 px-4">Buy Price</th>
                <th className="text-right py-3 px-4">Current Price</th>
                <th className="text-right py-3 px-4">PnL %</th>
                <th className="text-right py-3 px-4">PnL SOL</th>
                <th className="text-right py-3 px-4">Value</th>
                <th className="text-right py-3 px-4">Tokens</th>
                <th className="text-right py-3 px-4">Hold Time</th>
                <th className="text-center py-3 px-4">Link</th>
              </tr>
            </thead>
            <tbody>
              {stats.current_holdings.map((holding) => {
                const mint = holding.mint
                const symbol = holding.metadata?.symbol || holding.onchain?.symbol
                const name = holding.metadata?.name || holding.onchain?.name
                const image = holding.metadata?.image
                
                const liveData = prices[mint]
                const currentPrice = liveData ? liveData.price : holding.buy_price
                const decimals = holding.decimals || 6
                const tokenDivisor = Math.pow(10, decimals)
                const tokens = holding.amount / tokenDivisor
                // Always compute PnL locally from authoritative buy_price + latest price
                const buyCost = holding.buy_price * tokens
                const positionValue = currentPrice * tokens
                const pnlSol = positionValue - buyCost
                const pnlPercent = buyCost > 0 ? ((positionValue - buyCost) / buyCost) * 100 : 0
                
                const pnlClass = pnlPercent > 0.01 ? 'text-green-500' : pnlPercent < -0.01 ? 'text-red-500' : 'text-gray-400'
                const pnlSign = pnlPercent > 0 ? '+' : ''
                const pnlSolSign = pnlSol > 0 ? '+' : ''
                const isLive = !!liveData

                return (
                  <tr key={mint} className={`border-b border-gray-700/50 hover:bg-sol-darker/50 transition-all ${isLive ? '' : 'opacity-70'} ${isLive && pnlPercent > 0.01 ? 'profit-row-shimmer' : ''} ${isLive && pnlPercent < -0.01 ? 'loss-row' : ''}`}>
                    <td className="py-3 px-4">
                      <div className="flex items-center gap-2">
                        {image ? (
                          <img 
                            src={image} 
                            alt={symbol || name || 'Token'} 
                            className="w-8 h-8 rounded"
                            onError={(e) => {
                              e.currentTarget.style.display = 'none'
                            }}
                          />
                        ) : (
                          <div className="w-8 h-8 rounded bg-gray-700 flex items-center justify-center text-gray-500 text-xs">
                            ?
                          </div>
                        )}
                        <div>
                          <span className="font-mono text-xs text-gray-400">
                            {mint.slice(0, 6)}...{mint.slice(-4)}
                          </span>
                          {isLive && (
                            <span className="ml-2 inline-block w-2 h-2 rounded-full bg-green-400 animate-pulse" title="Live price feed" />
                          )}
                        </div>
                      </div>
                    </td>
                    <td className="py-3 px-4">
                      {name || symbol ? (
                        <div>
                          <div className="font-semibold">{name || symbol}</div>
                          {symbol && name && (
                            <div className="text-xs text-gray-500">${symbol}</div>
                          )}
                        </div>
                      ) : (
                        <span className="text-gray-500">Unknown</span>
                      )}
                    </td>
                    <td className="py-3 px-4 text-right font-mono text-xs">
                      {holding.buy_price.toFixed(12)} SOL
                    </td>
                    <td className="py-3 px-4 text-right font-mono text-xs">
                      <span className={isLive ? pnlClass : 'text-gray-500'}>
                        {currentPrice.toFixed(12)} SOL
                      </span>
                      {!isLive && <span className="ml-1 text-[10px] text-gray-600" title="Waiting for live price">(buy)</span>}
                    </td>
                    <td className={`py-3 px-4 text-right font-mono text-xs font-bold ${isLive ? pnlClass : 'text-gray-500'}`}>
                      {pnlSign}{pnlPercent.toFixed(2)}%
                    </td>
                    <td className={`py-3 px-4 text-right font-mono text-xs font-bold ${isLive ? pnlClass : 'text-gray-500'}`}>
                      {pnlSolSign}{pnlSol.toFixed(9)}
                    </td>
                    <td className="py-3 px-4 text-right font-mono text-xs text-blue-400">
                      {positionValue.toFixed(9)}
                    </td>
                    <td className="py-3 px-4 text-right font-mono">
                      {tokens.toLocaleString()}
                    </td>
                    <td className="py-3 px-4 text-right text-gray-400 text-xs font-mono">
                      {formatHoldTime(holding.buy_time)}
                    </td>
                    <td className="py-3 px-4 text-center">
                      <a
                        href={`https://solscan.io/token/${mint}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-sol-purple hover:text-sol-purple-light transition-colors inline-flex items-center gap-1"
                        title="View on Solscan"
                      >
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                          <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
                          <polyline points="15 3 21 3 21 9"></polyline>
                          <line x1="10" y1="14" x2="21" y2="3"></line>
                        </svg>
                      </a>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      </div>

      {/* Summary */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="card-enhanced rounded-xl p-4 hover:scale-105">
          <p className="text-gray-400 text-sm font-medium mb-2">Total Holdings</p>
          <p className="text-2xl font-bold gradient-text">
            {stats.current_holdings.length} positions
          </p>
        </div>

        <div className="card-enhanced rounded-xl p-4 hover:scale-105">
          <p className="text-gray-400 text-sm font-medium mb-2">Total Entry Cost</p>
          <p className="text-2xl font-bold text-blue-400">
            {totalBuyCostSol.toFixed(9)} SOL
          </p>
        </div>

        <div className="card-enhanced rounded-xl p-4 hover:scale-105">
          <p className="text-gray-400 text-sm font-medium mb-2">Unrealized PnL</p>
          <p className={`text-2xl font-bold ${totalPnlClass}`}>
            {totalPnlSol >= 0 ? '+' : ''}{totalPnlSol.toFixed(9)} SOL
          </p>
        </div>
      </div>
    </div>
  )
}
