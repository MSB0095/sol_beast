import { useState, useEffect } from 'react'
import { TrendingUp, TrendingDown, ExternalLink, Filter, Download, History } from 'lucide-react'
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
    
    // Poll every 2s for reasonable dashboard updates (100ms was too aggressive)
    const interval = setInterval(fetchTrades, 2000)
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
    const headers = ['Type', 'Time', 'Symbol', 'Mint', 'Amount SOL', 'Amount Tokens', 'Price', 'P/L', 'P/L %', 'Reason', 'TX']
    const escape = (v: unknown) => `"${String(v ?? '').replace(/"/g, '""')}"`
    const rows = filteredTrades.map(t => [
      t.type,
      t.timestamp,
      t.symbol || '',
      t.mint,
      typeof t.amount_sol === 'number' ? t.amount_sol.toFixed(4) : '',
      typeof t.amount_tokens === 'number' ? t.amount_tokens.toString() : '',
      typeof t.price_per_token === 'number' ? t.price_per_token.toFixed(9) : '',
      typeof t.profit_loss === 'number' ? t.profit_loss.toFixed(4) : '',
      typeof t.profit_loss_percent === 'number' ? t.profit_loss_percent.toFixed(2) : '',
      t.reason || '',
      t.tx_signature || ''
    ])

    const csv = [headers, ...rows].map(row => row.map(escape).join(',')).join('\n')
    const blob = new Blob([csv], { type: 'text/csv' })
    const url = window.URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `trades_${new Date().toISOString()}.csv`
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    window.URL.revokeObjectURL(url)
  }

  if (trades.length === 0) {
    return (
      <div className="card bg-base-200/50 border border-base-300 rounded-xl p-12 text-center">
        <div className="flex flex-col items-center gap-4">
          <div className="p-4 bg-base-100 rounded-full">
            <History className="w-12 h-12 text-base-content/50" />
          </div>
          <div>
            <h3 className="text-lg font-semibold text-base-content mb-2">No Trading History Yet</h3>
            <p className="text-base-content/60">Trades will appear here once executed</p>
          </div>
        </div>
      </div>
    )
  }

  // Prepare data for DataTable
  const tradesData = filteredTrades.map((trade, idx) => {
    const isProfit = (trade.profit_loss || 0) >= 0
    const isBuy = trade.type === 'buy'

    return {
      id: `${trade.mint}-${idx}`,
      type: (
        <div className="flex items-center gap-2">
          {isBuy ? (
            <div className="flex items-center gap-1 px-2 py-1 rounded-full text-xs font-semibold bg-success/20 text-success">
              <TrendingUp className="w-3 h-3" />
              BUY
            </div>
          ) : (
            <div className="flex items-center gap-1 px-2 py-1 rounded-full text-xs font-semibold bg-error/20 text-error">
              <TrendingDown className="w-3 h-3" />
              SELL
            </div>
          )}
        </div>
      ),
      time: (
        <div className="text-sm">
          <div className="font-mono text-base-content">
            {new Date(trade.timestamp).toLocaleDateString()}
          </div>
          <div className="text-xs text-base-content/60">
            {new Date(trade.timestamp).toLocaleTimeString()}
          </div>
        </div>
      ),
      token: (
        <div className="flex items-center gap-3">
          {trade.image && (
            <img
              src={trade.image}
              alt={trade.symbol || ''}
              className="w-8 h-8 rounded-lg object-cover"
              onError={(e) => {
                e.currentTarget.style.display = 'none'
              }}
            />
          )}
          <div>
            <div className="font-semibold text-base-content">
              {trade.symbol ? `$${trade.symbol}` : 'Unknown'}
            </div>
            <div className="text-xs text-base-content/60 font-mono">
              {trade.mint.slice(0, 8)}...
            </div>
          </div>
        </div>
      ),
      amount: (
        <div className="text-right font-mono">
          <div className="text-base-content">
            {trade.amount_sol ? trade.amount_sol.toFixed(4) : '-'}
          </div>
          <div className="text-xs text-base-content/60">SOL</div>
        </div>
      ),
      price: (
        <div className="text-right font-mono">
          <div className="text-base-content">
            {trade.price_per_token ? trade.price_per_token.toFixed(9) : '-'}
          </div>
          <div className="text-xs text-base-content/60">per token</div>
        </div>
      ),
      profitLoss: trade.profit_loss !== undefined && trade.profit_loss !== null ? (
        <div className="text-right">
          <div className={`font-semibold ${isProfit ? 'text-success' : 'text-error'}`}>
            {isProfit ? '+' : ''}{trade.profit_loss.toFixed(4)} SOL
          </div>
          <div className={`text-xs ${isProfit ? 'text-success' : 'text-error'}`}>
            {isProfit ? '+' : ''}{trade.profit_loss_percent !== null && trade.profit_loss_percent !== undefined ? trade.profit_loss_percent.toFixed(2) : '0'}%
          </div>
        </div>
      ) : (
        <span className="text-base-content/60">-</span>
      ),
      reason: (
        <div className="text-center">
          {trade.reason && (
            <span className={`px-2 py-1 rounded-full text-xs font-semibold ${
              trade.reason === 'TP'
                ? 'bg-success/20 text-success'
                : trade.reason === 'SL'
                ? 'bg-error/20 text-error'
                : 'bg-base-300 text-base-content/60'
            }`}>
              {trade.reason}
            </span>
          )}
        </div>
      ),
      tx: (
        <div className="text-center">
          {trade.tx_signature ? (
            <a
              href={`https://solscan.io/tx/${trade.tx_signature}`}
              target="_blank"
              rel="noopener noreferrer"
              className="btn btn-circle btn-text btn-sm"
              title="View on Solscan"
            >
              <ExternalLink className="w-4 h-4" />
            </a>
          ) : (
            <span className="text-base-content/60">-</span>
          )}
        </div>
      )
    }
  })

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="p-3 bg-info/10 rounded-lg">
          <History className="w-6 h-6 text-info" />
        </div>
        <div>
          <h3 className="text-xl font-bold text-base-content uppercase tracking-wider">
            Trading History
          </h3>
          <p className="text-base-content/60">
            {trades.length} completed trade{trades.length !== 1 ? 's' : ''}
          </p>
        </div>
      </div>

      {/* Controls */}
      <div className="card bg-base-200/50 border border-base-300 rounded-xl">
        <div className="card-body">
          {/* Filter Tabs */}
          <div className="flex flex-wrap gap-2 mb-4">
            {(['all', 'buy', 'sell'] as const).map((type) => (
              <button
                key={type}
                onClick={() => setFilter(type)}
                className={`btn btn-sm uppercase tracking-wider ${
                  filter === type
                    ? 'btn-primary'
                    : 'btn-soft btn-ghost'
                }`}
              >
                {type} ({type === 'all' ? trades.length : trades.filter(t => t.type === type).length})
              </button>
            ))}
          </div>

          <div className="flex flex-col md:flex-row gap-4">
            {/* Search */}
            <div className="form-control flex-1">
              <div className="input input-sm">
                <span className="icon-[tabler--search] text-base-content/80 my-auto me-3 size-4 shrink-0"></span>
                <input
                  type="search"
                  placeholder="Search by mint or symbol..."
                  value={searchTerm}
                  onChange={(e) => setSearchTerm(e.target.value)}
                  className="grow"
                />
              </div>
            </div>

            {/* Actions */}
            <div className="flex gap-2">
              <button
                onClick={() => setSortBy(sortBy === 'time' ? 'profit' : 'time')}
                className="btn btn-soft btn-sm gap-2"
              >
                <Filter className="w-4 h-4" />
                Sort by {sortBy === 'time' ? 'Time' : 'Profit'}
              </button>
              <button
                onClick={exportToCSV}
                className="btn btn-soft btn-sm gap-2"
              >
                <Download className="w-4 h-4" />
                Export CSV
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* Data Table */}
      <div
        className="bg-base-100 rounded-lg shadow-sm border border-base-300"
        data-datatable='{
          "pageLength": 15,
          "pagingOptions": {
            "pageBtnClasses": "btn btn-circle btn-sm"
          },
          "selecting": false,
          "language": {
            "zeroRecords": "<div class=\"py-8 text-center\"><History class=\"w-12 h-12 mx-auto mb-4 text-base-content/30\" /><p class=\"text-base-content/60\">No trades found</p></div>"
          }
        }'
      >
        <div className="overflow-x-auto">
          <table className="table table-zebra">
            <thead>
              <tr className="uppercase tracking-wide text-xs">
                <th className="w-24">Type</th>
                <th className="w-32">Time</th>
                <th className="w-48">Token</th>
                <th className="w-24 text-right">Amount</th>
                <th className="w-32 text-right">Price/Token</th>
                <th className="w-32 text-right">P/L</th>
                <th className="w-20 text-center">Reason</th>
                <th className="w-20 text-center">TX</th>
              </tr>
            </thead>
            <tbody>
              {tradesData.map((trade) => (
                <tr key={trade.id} className="hover:bg-base-200/50">
                  <td>{trade.type}</td>
                  <td>{trade.time}</td>
                  <td>{trade.token}</td>
                  <td>{trade.amount}</td>
                  <td>{trade.price}</td>
                  <td>{trade.profitLoss}</td>
                  <td>{trade.reason}</td>
                  <td>{trade.tx}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        
        {/* Pagination info */}
        <div className="border-t border-base-300 p-4 text-center">
          <p className="text-sm text-base-content/60">
            Showing {tradesData.length} of {tradesData.length} trades
          </p>
        </div>
      </div>

      {/* Summary Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="card bg-info/10 border border-info/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <History className="w-5 h-5 text-info" />
            <span className="text-sm font-medium text-info/80 uppercase">Total Trades</span>
          </div>
          <p className="text-2xl font-bold text-info">{trades.length}</p>
        </div>
        
        <div className="card bg-success/10 border border-success/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <TrendingUp className="w-5 h-5 text-success" />
            <span className="text-sm font-medium text-success/80 uppercase">Total Buys</span>
          </div>
          <p className="text-2xl font-bold text-success">
            {trades.filter(t => t.type === 'buy').length}
          </p>
        </div>
        
        <div className="card bg-error/10 border border-error/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <TrendingDown className="w-5 h-5 text-error" />
            <span className="text-sm font-medium text-error/80 uppercase">Total Sells</span>
          </div>
          <p className="text-2xl font-bold text-error">
            {trades.filter(t => t.type === 'sell').length}
          </p>
        </div>
        
        <div className={`card rounded-xl p-4 hover:scale-105 transition-transform ${
          trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0) >= 0
            ? 'bg-success/10 border border-success/20'
            : 'bg-error/10 border border-error/20'
        }`}>
          <div className="flex items-center gap-3 mb-2">
            <TrendingUp className={`w-5 h-5 ${
              trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0) >= 0 ? 'text-success' : 'text-error'
            }`} />
            <span className={`text-sm font-medium uppercase ${
              trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0) >= 0 ? 'text-success/80' : 'text-error/80'
            }`}>
              Total P/L
            </span>
          </div>
          <p className={`text-2xl font-bold ${
            trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0) >= 0
              ? 'text-success'
              : 'text-error'
          }`}>
            {trades.reduce((sum, t) => sum + (t.profit_loss || 0), 0).toFixed(4)} SOL
          </p>
        </div>
      </div>
    </div>
  )
}
