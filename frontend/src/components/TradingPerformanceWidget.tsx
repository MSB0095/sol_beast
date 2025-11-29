import { TrendingUp, BarChart3, Target } from 'lucide-react'
import { useBotStore } from '../store/botStore'
import { useSettingsStore } from '../store/settingsStore'
import { API_STATS_URL } from '../config'

export default function TradingPerformanceWidget() {
  const { stats, historicalData, updateStats } = useBotStore()

  // Summary metrics from backend-driven state
  const totalProfit = stats?.total_profit ?? 0
  const tradesCount = (stats ? (stats.total_buys || 0) + (stats.total_sells || 0) : 0)

  // Average change computed from last N historical points
  const recent = historicalData.slice(-5)
  const avgChange = recent.length
    ? (
        recent.reduce((sum, d) => sum + (d.profit ?? 0), 0) / recent.length
      ).toFixed(2)
    : '0.00'

  // Top holdings list (derive from current_holdings)
  // Sort top holdings by estimated position value (amount * buy_price) descending
  const topHoldings = (stats?.current_holdings || [])
    .map(h => ({
      ...h,
      __estimated_value: (typeof h.amount === 'number' && typeof h.buy_price === 'number') ? (h.amount * h.buy_price) : 0
    }))
    .sort((a, b) => (b.__estimated_value || 0) - (a.__estimated_value || 0))
    .slice(0, 5)

  const handleRefresh = async () => {
    try {
      const res = await fetch(API_STATS_URL)
      if (res.ok) {
        const data = await res.json()
        updateStats(data)
      }
    } catch (err) {
      console.error('Failed to refresh stats', err)
    }
  }

  return (
    <div className="card bg-base-200/50 border border-base-300 rounded-xl">
      <div className="card-body">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div className="flex items-center gap-3">
            <div className="p-3 bg-primary/10 rounded-lg">
              <BarChart3 className="w-6 h-6 text-primary" />
            </div>
            <div>
              <h4 className="text-xl font-bold text-base-content uppercase tracking-wider">
                Top Performing Tokens
              </h4>
              <p className="text-base-content/60">Real-time performance tracking</p>
            </div>
          </div>
          <button onClick={handleRefresh} className="btn btn-soft btn-sm gap-2">
            <Target className="w-4 h-4" />
            Refresh
          </button>
        </div>

        {/* Summary Statistics */}
        <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-6">
          <div className="card bg-success/10 border border-success/20 rounded-lg p-3">
            <div className="flex items-center gap-2 mb-1">
              <TrendingUp className="w-4 h-4 text-success" />
              <span className="text-xs font-medium text-success/80 uppercase">Total Profit</span>
            </div>
            <p className="text-lg font-bold text-success">{totalProfit.toFixed(3)} SOL</p>
          </div>

          <div className="card bg-info/10 border border-info/20 rounded-lg p-3">
            <div className="flex items-center gap-2 mb-1">
              <Target className="w-4 h-4 text-info" />
              <span className="text-xs font-medium text-info/80 uppercase">Trades</span>
            </div>
            <p className="text-lg font-bold text-info">{tradesCount}</p>
          </div>

          <div className="card bg-warning/10 border border-warning/20 rounded-lg p-3">
            <div className="flex items-center gap-2 mb-1">
              <BarChart3 className="w-4 h-4 text-warning" />
              <span className="text-xs font-medium text-warning/80 uppercase">Avg Profit</span>
            </div>
            <p className="text-lg font-bold text-warning">{avgChange}%</p>
          </div>

          <div className="card bg-secondary/10 border border-secondary/20 rounded-lg p-3">
            <div className="flex items-center gap-2 mb-1">
              <TrendingUp className="w-4 h-4 text-secondary" />
              <span className="text-xs font-medium text-secondary/80 uppercase">Holdings</span>
            </div>
            <p className="text-lg font-bold text-secondary">{(stats?.current_holdings || []).length}</p>
          </div>
        </div>

        {/* Performance List (driven by backend holdings) */}
        <div className="space-y-3">
          {topHoldings.map((h, index) => (
            <div key={h.mint} className="card bg-base-100 border border-base-300 hover:border-primary/50 transition-all duration-300">
              <div className="card-body p-4">
                <div className="flex items-center gap-4">
                  <div
                    className="w-12 h-12 rounded-full flex items-center justify-center font-bold text-white relative overflow-hidden group-hover:scale-110 transition-transform"
                    style={{
                      background: `linear-gradient(135deg, var(--brand-${(index % 5) + 1}), var(--brand-${(index % 5) + 1}-muted))`,
                      boxShadow: `0 0 12px var(--brand-${(index % 5) + 1}-muted)`
                    }}
                  >
                    <span className="text-sm font-bold">{(h.metadata?.symbol || h.onchain?.symbol || '').slice(0, 6) || 'TOK'}</span>
                    <div className="absolute inset-0 rounded-full border-2 opacity-40" style={{ borderColor: `var(--brand-${(index % 5) + 1})`, opacity: 0.38 }} />
                  </div>

                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-3 mb-1">
                      <h6 className="font-bold text-base-content">◎{(h.buy_price ?? 0).toFixed(6)}</h6>
                      <div className={`flex items-center gap-1 text-base-content/60`}>
                        <span className="text-sm font-semibold">{((h.amount || 0) / 1_000_000).toLocaleString()} units</span>
                      </div>
                    </div>
                    <p className="text-base-content/60 text-sm">{h.metadata?.name || h.onchain?.name || h.metadata?.symbol || h.onchain?.symbol}</p>
                  </div>

                  <div className="text-right">
                    <span className="badge badge-soft badge-primary">
                      {new Date(h.buy_time).toLocaleTimeString()}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          ))}
        </div>

        {/* View All Button */}
        <div className="mt-6 pt-4 border-t border-base-300">
          <button
            onClick={() => useSettingsStore.getState().setActiveTab('trades')}
            className="btn btn-primary btn-wide gap-2"
          >
            <BarChart3 className="w-4 h-4" />
            View All Trades
          </button>
        </div>
      </div>
    </div>
  )
}
