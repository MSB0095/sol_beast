import { useBotStore } from '../store/botStore'
import { Clock, Coins, TrendingUp, ExternalLink } from 'lucide-react'

export default function HoldingsPanel() {
  const { stats } = useBotStore()

  if (!stats?.current_holdings || stats.current_holdings.length === 0) {
    return (
      <div className="card bg-base-200/50 border border-base-300 rounded-xl p-12 text-center">
        <div className="flex flex-col items-center gap-4">
          <div className="p-4 bg-base-100 rounded-full">
            <Clock className="w-12 h-12 text-base-content/50" />
          </div>
          <div>
            <h3 className="text-lg font-semibold text-base-content mb-2">No Active Holdings</h3>
            <p className="text-base-content/60">Trades will appear here when they're active</p>
          </div>
        </div>
      </div>
    )
  }

  // Prepare data for DataTable
  const holdingsData = stats.current_holdings.map((holding) => {
    const mint = holding.mint
    const symbol = holding.metadata?.symbol || holding.onchain?.symbol
    const name = holding.metadata?.name || holding.onchain?.name
    const image = holding.metadata?.image
    const holdTime = Math.floor((Date.now() - new Date(holding.buy_time).getTime()) / 1000)
    const minutes = Math.floor(holdTime / 60)
    const seconds = holdTime % 60

    return {
      id: mint,
      token: image ? (
        <div className="flex items-center gap-3">
          <img
            src={image}
            alt={symbol || name || 'Token'}
            className="w-10 h-10 rounded-lg object-cover"
            onError={(e) => {
              e.currentTarget.style.display = 'none'
            }}
          />
          <div className="font-mono text-xs text-base-content/60">
            {mint.slice(0, 6)}...{mint.slice(-4)}
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-lg bg-base-300 flex items-center justify-center">
            <Coins className="w-5 h-5 text-base-content/50" />
          </div>
          <div className="font-mono text-xs text-base-content/60">
            {mint.slice(0, 6)}...{mint.slice(-4)}
          </div>
        </div>
      ),
      name: name || symbol ? (
        <div>
          <div className="font-semibold text-base-content">{name || symbol}</div>
          {symbol && name && (
            <div className="text-xs text-base-content/60">${symbol}</div>
          )}
        </div>
      ) : (
        <span className="text-base-content/60">Unknown</span>
      ),
      buyPrice: (
        <div className="text-right font-mono">
          <div className="text-base-content">{typeof holding.buy_price === 'number' ? holding.buy_price.toFixed(9) : '-'}</div>
          <div className="text-xs text-base-content/60">SOL</div>
        </div>
      ),
      tokens: (
        <div className="text-right font-mono">
          <div className="text-base-content">{(typeof holding.amount === 'number' ? (holding.amount / 1_000_000) : 0).toLocaleString()}</div>
          <div className="text-xs text-base-content/60">units</div>
        </div>
      ),
      holdTime: (
        <div className="text-right">
          <div className="text-base-content font-mono">{minutes}m {seconds}s</div>
          <div className="text-xs text-base-content/60">active</div>
        </div>
      ),
      actions: (
        <div className="flex justify-center">
          <a
            href={`https://solscan.io/token/${mint}`}
            target="_blank"
            rel="noopener noreferrer"
            className="btn btn-circle btn-text btn-sm"
            title="View on Solscan"
          >
            <ExternalLink className="w-4 h-4" />
          </a>
        </div>
      )
    }
  })

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="p-3 bg-success/10 rounded-lg">
          <TrendingUp className="w-6 h-6 text-success" />
        </div>
        <div>
          <h3 className="text-xl font-bold text-base-content uppercase tracking-wider">
            Current Holdings
          </h3>
          <p className="text-base-content/60">
            {stats.current_holdings.length} active position{stats.current_holdings.length !== 1 ? 's' : ''}
          </p>
        </div>
      </div>

      {/* Data Table */}
      <div
        className="bg-base-100 rounded-lg shadow-sm border border-base-300"
        data-datatable='{
          "pageLength": 10,
          "pagingOptions": {
            "pageBtnClasses": "btn btn-circle btn-sm"
          },
          "selecting": false,
          "language": {
            "zeroRecords": "<div class=\"py-8 text-center\"><Coins class=\"w-12 h-12 mx-auto mb-4 text-base-content/30\" /><p class=\"text-base-content/60\">No holdings found</p></div>"
          }
        }'
      >
        <div className="overflow-x-auto">
          <table className="table table-zebra">
            <thead>
              <tr className="uppercase tracking-wide text-xs">
                <th className="w-48">Token</th>
                <th>Name/Symbol</th>
                <th className="w-32 text-right">Buy Price</th>
                <th className="w-32 text-right">Tokens</th>
                <th className="w-32 text-right">Hold Time</th>
                <th className="w-20 text-center">Actions</th>
              </tr>
            </thead>
            <tbody>
              {holdingsData.map((holding) => (
                <tr key={holding.id} className="hover:bg-base-200/50">
                  <td>{holding.token}</td>
                  <td>{holding.name}</td>
                  <td>{holding.buyPrice}</td>
                  <td>{holding.tokens}</td>
                  <td>{holding.holdTime}</td>
                  <td>{holding.actions}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        
        {/* Pagination info */}
        <div className="border-t border-base-300 p-4 text-center">
          <p className="text-sm text-base-content/60">
            Showing {holdingsData.length} of {holdingsData.length} holdings
          </p>
        </div>
      </div>

      {/* Summary Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="card bg-success/10 border border-success/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <Coins className="w-5 h-5 text-success" />
            <span className="text-sm font-medium text-success/80 uppercase">Total Holdings</span>
          </div>
          <p className="text-2xl font-bold text-success">
            {stats.current_holdings.length} positions
          </p>
        </div>

        <div className="card bg-info/10 border border-info/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <TrendingUp className="w-5 h-5 text-info" />
            <span className="text-sm font-medium text-info/80 uppercase">Avg Entry Price</span>
          </div>
          <p className="text-2xl font-bold text-info">
            {(() => {
              const len = stats.current_holdings.length
              if (len === 0) return '0.000000000'
              const sum = stats.current_holdings.reduce((sum, h) => sum + (typeof h.buy_price === 'number' ? h.buy_price : 0), 0)
              return (sum / len).toFixed(9)
            })()} SOL
          </p>
        </div>

        <div className="card bg-primary/10 border border-primary/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <Clock className="w-5 h-5 text-primary" />
            <span className="text-sm font-medium text-primary/80 uppercase">Total Tokens</span>
          </div>
          <p className="text-2xl font-bold text-primary">
            {(
              stats.current_holdings.reduce((sum, h) => sum + (typeof h.amount === 'number' ? h.amount : 0), 0) / 1_000_000
            ).toLocaleString()}
          </p>
        </div>
      </div>
    </div>
  )
}
