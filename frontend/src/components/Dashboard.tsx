import { useBotStore } from '../store/botStore'
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, BarChart, Bar } from 'recharts'
import { TrendingUp, TrendingDown, Target } from 'lucide-react'

export default function Dashboard() {
  const { stats } = useBotStore()

  const mockChartData = [
    { time: '00:00', profit: 0, trades: 0 },
    { time: '04:00', profit: 150, trades: 5 },
    { time: '08:00', profit: 280, trades: 12 },
    { time: '12:00', profit: 420, trades: 18 },
    { time: '16:00', profit: 550, trades: 25 },
    { time: '20:00', profit: 750, trades: 35 },
  ]

  return (
    <div className="space-y-6">
      {/* Key Metrics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="bg-sol-dark rounded-lg border border-gray-700 p-6">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-gray-400 text-sm">Total Profit</p>
              <h3 className={`text-3xl font-bold mt-2 ${(stats?.total_profit || 0) >= 0 ? 'text-green-400' : 'text-red-400'}`}>
                ${(stats?.total_profit || 0).toFixed(2)}
              </h3>
            </div>
            {(stats?.total_profit || 0) >= 0 ? (
              <TrendingUp size={32} className="text-green-400 opacity-30" />
            ) : (
              <TrendingDown size={32} className="text-red-400 opacity-30" />
            )}
          </div>
        </div>

        <div className="bg-sol-dark rounded-lg border border-gray-700 p-6">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-gray-400 text-sm">Total Trades</p>
              <h3 className="text-3xl font-bold mt-2 text-sol-purple">
                {(stats?.total_buys || 0) + (stats?.total_sells || 0)}
              </h3>
            </div>
            <Target size={32} className="text-sol-purple opacity-30" />
          </div>
        </div>

        <div className="bg-sol-dark rounded-lg border border-gray-700 p-6">
          <p className="text-gray-400 text-sm">Active Holdings</p>
          <h3 className="text-3xl font-bold mt-2 text-blue-400">
            {stats?.current_holdings?.length || 0}
          </h3>
          <p className="text-xs text-gray-500 mt-2">
            Max: {stats?.current_holdings?.length || 0} holdings
          </p>
        </div>
      </div>

      {/* Charts */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-sol-dark rounded-lg border border-gray-700 p-6">
          <h3 className="text-lg font-semibold mb-4">Profit Over Time</h3>
          <ResponsiveContainer width="100%" height={300}>
            <LineChart data={mockChartData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
              <XAxis dataKey="time" stroke="#9CA3AF" />
              <YAxis stroke="#9CA3AF" />
              <Tooltip 
                contentStyle={{ backgroundColor: '#1a1d20', border: '1px solid #374151', borderRadius: '0.5rem' }}
                cursor={{ stroke: '#14F195' }}
              />
              <Line type="monotone" dataKey="profit" stroke="#14F195" strokeWidth={2} dot={false} />
            </LineChart>
          </ResponsiveContainer>
        </div>

        <div className="bg-sol-dark rounded-lg border border-gray-700 p-6">
          <h3 className="text-lg font-semibold mb-4">Trade Activity</h3>
          <ResponsiveContainer width="100%" height={300}>
            <BarChart data={mockChartData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#374151" />
              <XAxis dataKey="time" stroke="#9CA3AF" />
              <YAxis stroke="#9CA3AF" />
              <Tooltip 
                contentStyle={{ backgroundColor: '#1a1d20', border: '1px solid #374151', borderRadius: '0.5rem' }}
              />
              <Bar dataKey="trades" fill="#14F195" radius={[8, 8, 0, 0]} />
            </BarChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Status Info */}
      <div className="bg-sol-dark rounded-lg border border-gray-700 p-6">
        <h3 className="text-lg font-semibold mb-4">Bot Information</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
          <div>
            <p className="text-gray-400">Buys Executed</p>
            <p className="text-2xl font-bold text-sol-purple">{stats?.total_buys || 0}</p>
          </div>
          <div>
            <p className="text-gray-400">Sells Executed</p>
            <p className="text-2xl font-bold text-sol-purple">{stats?.total_sells || 0}</p>
          </div>
          <div>
            <p className="text-gray-400">Win Rate</p>
            <p className="text-2xl font-bold text-sol-purple">
              {stats?.total_buys ? Math.round((stats.total_sells / stats.total_buys) * 100) : 0}%
            </p>
          </div>
          <div>
            <p className="text-gray-400">Uptime</p>
            <p className="text-2xl font-bold text-sol-purple">
              {stats?.uptime_secs ? (stats.uptime_secs / 3600).toFixed(1) : 0}h
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
