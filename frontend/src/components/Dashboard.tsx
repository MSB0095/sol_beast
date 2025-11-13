import { useEffect, useMemo } from 'react'
import { useBotStore } from '../store/botStore'
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, BarChart, Bar } from 'recharts'
import { TrendingUp, TrendingDown, Target, Loader } from 'lucide-react'

export default function Dashboard() {
  const { stats, historicalData } = useBotStore()

  // Generate chart data from historical data
  const chartData = useMemo(() => {
    if (historicalData.length === 0) {
      return []
    }
    
    // Convert historical data to chart format with relative timestamps
    return historicalData.map((point, index) => ({
      time: index,
      profit: point.profit,
      trades: point.trades,
      originalTime: new Date(point.timestamp).toLocaleTimeString(),
    }))
  }, [historicalData])

  // Ensure component re-renders when stats or historicalData changes
  useEffect(() => {
    // This effect ensures the component subscribes to store updates
    // The dependency on stats and historicalData will cause re-render when they change
  }, [stats, historicalData])

  if (!stats) {
    return (
      <div className="flex items-center justify-center py-12">
        <Loader size={32} className="animate-spin text-sol-purple" />
        <span className="ml-3 text-gray-400">Loading statistics...</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Key Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="card-enhanced rounded-xl p-6 group hover:scale-105">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-gray-400 text-sm font-medium mb-2">Total Profit</p>
              <h3 className={`text-3xl font-bold ${(stats?.total_profit || 0) >= 0 ? 'text-green-400' : 'text-red-400'}`}>
                ◎{(stats?.total_profit || 0).toFixed(9)}
              </h3>
            </div>
            {(stats?.total_profit || 0) >= 0 ? (
              <div className="p-3 rounded-xl bg-green-500/10 border border-green-500/20">
                <TrendingUp size={32} className="text-green-400" />
              </div>
            ) : (
              <div className="p-3 rounded-xl bg-red-500/10 border border-red-500/20">
                <TrendingDown size={32} className="text-red-400" />
              </div>
            )}
          </div>
        </div>

        <div className="card-enhanced rounded-xl p-6 group hover:scale-105">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-gray-400 text-sm font-medium mb-2">Total Trades</p>
              <h3 className="text-3xl font-bold gradient-text">
                {(stats?.total_buys || 0) + (stats?.total_sells || 0)}
              </h3>
            </div>
            <div className="p-3 rounded-xl bg-sol-purple/10 border border-sol-purple/20">
              <Target size={32} className="text-sol-purple" />
            </div>
          </div>
        </div>

        <div className="card-enhanced rounded-xl p-6 group hover:scale-105">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-gray-400 text-sm font-medium mb-2">Active Holdings</p>
              <h3 className="text-3xl font-bold text-blue-400">
                {stats?.current_holdings?.length || 0}
              </h3>
              <p className="text-xs text-gray-500 mt-2 font-medium">
                Buys: {stats?.total_buys || 0} | Sells: {stats?.total_sells || 0}
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="card-enhanced rounded-xl p-6">
          <h3 className="text-lg font-semibold mb-4 gradient-text">Profit Over Time</h3>
          <ResponsiveContainer width="100%" height={300}>
            <LineChart data={chartData.length > 0 ? chartData : [{ time: 0, profit: 0, trades: 0 }]}>
              <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
              <XAxis 
                dataKey="time" 
                stroke="#9CA3AF"
                tick={{ fontSize: 12 }}
              />
              <YAxis stroke="#9CA3AF" />
              <Tooltip 
                contentStyle={{ backgroundColor: '#1a1d20', border: '1px solid #374151', borderRadius: '0.5rem' }}
                cursor={{ stroke: '#14F195' }}
                formatter={(value: any) => value.toFixed(9)}
                labelFormatter={(label) => `Point ${label}`}
              />
              <Line 
                type="monotone" 
                dataKey="profit" 
                stroke="#14F195" 
                strokeWidth={2} 
                dot={false}
                isAnimationActive={false}
              />
            </LineChart>
          </ResponsiveContainer>
        </div>

        <div className="card-enhanced rounded-xl p-6">
          <h3 className="text-lg font-semibold mb-4 gradient-text">Trade Activity</h3>
          <ResponsiveContainer width="100%" height={300}>
            <BarChart data={chartData.length > 0 ? chartData : [{ time: 0, profit: 0, trades: 0 }]}>
              <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
              <XAxis 
                dataKey="time" 
                stroke="#9CA3AF"
                tick={{ fontSize: 12 }}
              />
              <YAxis stroke="#9CA3AF" />
              <Tooltip 
                contentStyle={{ backgroundColor: '#1a1d20', border: '1px solid #374151', borderRadius: '0.5rem' }}
                labelFormatter={(label) => `Point ${label}`}
              />
              <Bar dataKey="trades" fill="#14F195" radius={[8, 8, 0, 0]} isAnimationActive={false} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Status Info */}
      <div className="card-enhanced rounded-xl p-6">
        <h3 className="text-lg font-semibold mb-4 gradient-text">Bot Information</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
          <div className="p-4 rounded-xl bg-sol-darker/50 border border-gray-700/30">
            <p className="text-gray-400 font-medium mb-2">Buys Executed</p>
            <p className="text-2xl font-bold gradient-text">{stats?.total_buys || 0}</p>
          </div>
          <div className="p-4 rounded-xl bg-sol-darker/50 border border-gray-700/30">
            <p className="text-gray-400 font-medium mb-2">Sells Executed</p>
            <p className="text-2xl font-bold gradient-text">{stats?.total_sells || 0}</p>
          </div>
          <div className="p-4 rounded-xl bg-sol-darker/50 border border-gray-700/30">
            <p className="text-gray-400 font-medium mb-2">Win Rate</p>
            <p className="text-2xl font-bold gradient-text">
              {stats?.total_buys && stats.total_buys > 0 ? Math.round((stats.total_sells / stats.total_buys) * 100) : 0}%
            </p>
          </div>
          <div className="p-4 rounded-xl bg-sol-darker/50 border border-gray-700/30">
            <p className="text-gray-400 font-medium mb-2">Uptime</p>
            <p className="text-2xl font-bold gradient-text">
              {stats?.uptime_secs ? (stats.uptime_secs / 3600).toFixed(1) : 0}h
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
