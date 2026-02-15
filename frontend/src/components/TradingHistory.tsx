import { useState, useEffect } from 'react'
import { TrendingUp, TrendingDown, ExternalLink, Search, Filter, Download } from 'lucide-react'
import { API_TRADES_URL } from '../config'

interface Trade {
  mint: string
  symbol?: string
  name?: string
  image?: string
  type: 'buy' | 'sell'
  timestamp: string
  tx_signature?: string
  amount_sol: number
  amount_tokens: number
  price_per_token: number
  profit_loss?: number
  profit_loss_percent?: number
  reason?: 'TP' | 'SL' | 'TIMEOUT' | 'MANUAL'
  decimals?: number
  actual_sol_change?: number
  tx_fee_sol?: number
  simulated?: boolean
}

export default function TradingHistory() {
  const [trades, setTrades] = useState<Trade[]>([])
  const [filter, setFilter] = useState<'all' | 'buy' | 'sell'>('all')
  const [searchTerm, setSearchTerm] = useState('')
  const [sortBy, setSortBy] = useState<'time' | 'profit'>('time')

  useEffect(() => {
    const fetchTrades = async () => {
      try {
        const response = await fetch(API_TRADES_URL)
        if (response.ok) {
          const data = await response.json()
          setTrades(data)
        }
      } catch (error) {
        console.error('Failed to fetch trades:', error)
      }
    }
    
    // Fetch immediately
    fetchTrades()
    
    // Poll every 100ms for near-instant updates
    const interval = setInterval(fetchTrades, 100)
    return () => clearInterval(interval)
  }, [])

  const filteredTrades = trades
    .filter(trade => {
      if (filter !== 'all' && trade.type !== filter) return false
      if (searchTerm && !trade.mint.includes(searchTerm) && !trade.symbol?.includes(searchTerm)) return false
      return true
    })
    .sort((a, b) => {
      if (sortBy === 'time') {
        return new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
      } else {
        return (b.profit_loss || 0) - (a.profit_loss || 0)
      }
    })

  const exportToCSV = () => {
    const headers = ['Type', 'Time', 'Symbol', 'Mint', 'Amount SOL', 'On-Chain SOL Change', 'Amount Tokens', 'Price', 'P/L', 'P/L %', 'TX Fee', 'Reason', 'TX']
    const rows = filteredTrades.map(t => [
      t.type,
      t.timestamp,
      t.symbol || '',
      t.mint,
      t.amount_sol,
      t.actual_sol_change ?? '',
      t.amount_tokens,
      t.price_per_token,
      t.profit_loss || '',
      t.profit_loss_percent || '',
      t.tx_fee_sol ?? '',
      t.reason || '',
      t.tx_signature || ''
    ])
    
    const csv = [headers, ...rows].map(row => row.join(',')).join('\n')
    const blob = new Blob([csv], { type: 'text/csv' })
    const url = window.URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `trades_${new Date().toISOString()}.csv`
    a.click()
  }

  if (trades.length === 0) {
    return (
      <div className="cyber-card rounded-xl p-12 text-center" style={{ backgroundColor: 'var(--theme-bg-card)' }}>
        <TrendingUp size={48} className="mx-auto mb-4 opacity-50" style={{ color: 'var(--theme-text-muted)' }} />
        <p className="uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>No trading history yet</p>
        <p className="text-sm font-mono" style={{ color: 'var(--theme-text-muted)' }}>Trades will appear here once executed</p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Controls */}
      <div className="cyber-card rounded-xl p-4" style={{ backgroundColor: 'var(--theme-bg-card)' }}>
        <div className="flex flex-wrap gap-4 items-center justify-between">
          {/* Filter Tabs */}
          <div className="flex gap-2">
            {(['all', 'buy', 'sell'] as const).map((type) => (
              <button
                key={type}
                onClick={() => setFilter(type)}
                className="px-4 py-2 rounded-lg transition-colors uppercase tracking-wider font-semibold"
                style={filter === type ? {
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
                {type} ({type === 'all' ? trades.length : trades.filter(t => t.type === type).length})
              </button>
            ))}
          </div>

          {/* Search */}
          <div className="flex gap-2 flex-1 max-w-md">
            <div className="relative flex-1">
              <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--theme-text-muted)' }} />
              <input
                type="text"
                placeholder="Search by mint or symbol..."
                value={searchTerm}
                onChange={(e) => setSearchTerm(e.target.value)}
                className="w-full rounded-lg pl-10 pr-4 py-2 text-sm"
              />
            </div>
          </div>

          {/* Sort & Export */}
          <div className="flex gap-2">
            <button
              onClick={() => setSortBy(sortBy === 'time' ? 'profit' : 'time')}
              className="flex items-center gap-2 px-4 py-2 rounded-lg transition-colors uppercase tracking-wide"
              style={{
                backgroundColor: 'var(--theme-bg-secondary)',
                color: 'var(--theme-text-secondary)',
                border: '1px solid var(--theme-accent)'
              }}
            >
              <Filter size={16} />
              Sort by {sortBy === 'time' ? 'Time' : 'Profit'}
            </button>
            <button
              onClick={exportToCSV}
              className="flex items-center gap-2 px-4 py-2 rounded-lg transition-colors uppercase tracking-wide"
            >
              <Download size={16} />
              Export CSV
            </button>
          </div>
        </div>
      </div>

      {/* Trades Table */}
      <div className="cyber-card rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--theme-bg-card)' }}>
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b-2 uppercase tracking-wider font-semibold" style={{ 
                color: 'var(--theme-text-secondary)',
                backgroundColor: 'var(--theme-bg-secondary)',
                borderColor: 'var(--theme-accent)'
              }}>
                <th className="text-left py-3 px-4">Type</th>
                <th className="text-left py-3 px-4">Time</th>
                <th className="text-left py-3 px-4">Token</th>
                <th className="text-right py-3 px-4">Amount (SOL)</th>
                <th className="text-right py-3 px-4">On-Chain Δ</th>
                <th className="text-right py-3 px-4">Price/Token</th>
                <th className="text-right py-3 px-4">P/L</th>
                <th className="text-right py-3 px-4">Fee</th>
                <th className="text-center py-3 px-4">Reason</th>
                <th className="text-center py-3 px-4">TX</th>
              </tr>
            </thead>
            <tbody>
              {filteredTrades.map((trade, idx) => {
                const isProfit = (trade.profit_loss || 0) >= 0
                const isBuy = trade.type === 'buy'

                return (
                  <tr key={`${trade.mint}-${idx}`} className={`border-b border-gray-700 hover:bg-sol-darker transition-all ${
                    !isBuy && isProfit ? 'profit-row-shimmer' : ''
                  } ${!isBuy && !isProfit ? 'loss-row' : ''}`}
                  >
                    <td className="py-3 px-4">
                      <span className={`inline-flex items-center gap-1 px-2 py-1 rounded text-xs font-semibold ${
                        isBuy 
                          ? 'bg-green-900/30 text-green-400' 
                          : 'bg-red-900/30 text-red-400'
                      }`}>
                        {isBuy ? (
                          <>
                            <TrendingUp size={12} />
                            BUY
                          </>
                        ) : (
                          <>
                            <TrendingDown size={12} />
                            SELL
                          </>
                        )}
                      </span>
                      {trade.simulated && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-bold bg-yellow-900/40 text-yellow-400 border border-yellow-600/40 ml-1">
                          SIM
                        </span>
                      )}
                    </td>
                    
                    <td className="py-3 px-4 text-gray-400 text-xs">
                      {new Date(trade.timestamp).toLocaleString()}
                    </td>
                    
                    <td className="py-3 px-4">
                      <div className="flex items-center gap-2">
                        {trade.image && (
                          <img 
                            src={trade.image} 
                            alt={trade.symbol || ''} 
                            className="w-6 h-6 rounded"
                            onError={(e) => {
                              e.currentTarget.style.display = 'none'
                            }}
                          />
                        )}
                        <div>
                          <div className="font-semibold">
                            {trade.symbol ? `$${trade.symbol}` : 'Unknown'}
                          </div>
                          <div className="text-xs text-gray-500 font-mono">
                            {trade.mint.slice(0, 8)}...
                          </div>
                        </div>
                      </div>
                    </td>
                    
                    <td className="py-3 px-4 text-right font-mono">
                      {trade.amount_sol ? trade.amount_sol.toFixed(4) : '-'}
                    </td>
                    
                    <td className="py-3 px-4 text-right font-mono text-xs">
                      {trade.actual_sol_change !== undefined && trade.actual_sol_change !== null ? (
                        <span className={trade.actual_sol_change >= 0 ? 'text-green-400' : 'text-red-400'}>
                          {trade.actual_sol_change >= 0 ? '+' : ''}{trade.actual_sol_change.toFixed(6)}
                        </span>
                      ) : (
                        <span className="text-gray-500 italic">est.</span>
                      )}
                    </td>
                    
                    <td className="py-3 px-4 text-right font-mono text-xs">
                      {trade.price_per_token ? trade.price_per_token.toFixed(9) : '-'}
                    </td>
                    
                    <td className="py-3 px-4 text-right">
                      {trade.profit_loss !== undefined && trade.profit_loss !== null ? (
                        <div className={isProfit ? 'animate-profit-number' : ''}>
                          <div className={`font-semibold ${isProfit ? 'text-green-400' : 'text-red-400'}`}>
                            {isProfit ? '+' : ''}{trade.profit_loss.toFixed(4)} SOL
                          </div>
                          <div className={`text-xs ${isProfit ? 'text-green-400' : 'text-red-400'}`}>
                            {isProfit ? '+' : ''}{trade.profit_loss_percent !== null && trade.profit_loss_percent !== undefined ? trade.profit_loss_percent.toFixed(2) : '0'}%
                          </div>
                        </div>
                      ) : (
                        <span className="text-gray-500">-</span>
                      )}
                    </td>
                    
                    <td className="py-3 px-4 text-right font-mono text-xs">
                      {trade.tx_fee_sol !== undefined && trade.tx_fee_sol !== null ? (
                        <span className="text-yellow-400">{trade.tx_fee_sol.toFixed(6)}</span>
                      ) : (
                        <span className="text-gray-500">-</span>
                      )}
                    </td>
                    
                    <td className="py-3 px-4 text-center">
                      {trade.reason && (
                        <span className={`px-2 py-1 rounded text-xs font-semibold ${
                          trade.reason === 'TP' 
                            ? 'bg-green-900/30 text-green-400' 
                            : trade.reason === 'SL'
                            ? 'bg-red-900/30 text-red-400'
                            : 'bg-gray-700 text-gray-400'
                        }`}>
                          {trade.reason}
                        </span>
                      )}
                    </td>
                    
                    <td className="py-3 px-4 text-center">
                      {trade.tx_signature ? (
                        <a
                          href={`https://solscan.io/tx/${trade.tx_signature}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="inline-flex items-center gap-1 text-sol-purple hover:text-sol-purple-light transition-colors"
                          title="View on Solscan"
                        >
                          <ExternalLink size={16} />
                        </a>
                      ) : (
                        <span className="text-gray-500">-</span>
                      )}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      </div>

      {/* Summary Stats */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm">Total Trades</p>
          <p className="text-2xl font-bold text-sol-purple mt-2">{trades.length}</p>
        </div>
        
        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm">Total Buys</p>
          <p className="text-2xl font-bold text-green-400 mt-2">
            {trades.filter(t => t.type === 'buy').length}
          </p>
        </div>
        
        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm">Total Sells</p>
          <p className="text-2xl font-bold text-red-400 mt-2">
            {trades.filter(t => t.type === 'sell').length}
          </p>
        </div>
        
        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm">Total P/L</p>
          <p className={`text-2xl font-bold mt-2 ${
            trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0) >= 0 
              ? 'text-green-400' 
              : 'text-red-400'
          }`}>
            {trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0).toFixed(4)} SOL
          </p>
        </div>
      </div>
    </div>
  )
}
