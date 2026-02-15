import { useEffect, useMemo, useRef, useState } from 'react'
import { useBotStore } from '../store/botStore'
import { XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, BarChart, Bar, Area, AreaChart } from 'recharts'
import { TrendingUp, TrendingDown, Target, Loader, Wallet, Sparkles } from 'lucide-react'
import TradingPerformanceWidget from './TradingPerformanceWidget'

export default function Dashboard() {
  const { stats, historicalData, detectedCoins, totalDetectedCoins } = useBotStore()
  const prevProfitRef = useRef<number>(0)
  const [celebrating, setCelebrating] = useState(false)
  const [sparkles, setSparkles] = useState<{id: number; x: number; y: number}[]>([])

  // Detect profit changes for celebration animation
  useEffect(() => {
    const currentProfit = stats?.total_profit || 0
    const prevProfit = prevProfitRef.current
    if (currentProfit > prevProfit && prevProfit !== 0) {
      setCelebrating(true)
      // Spawn sparkle particles
      const newSparkles = Array.from({ length: 6 }, (_, i) => ({
        id: Date.now() + i,
        x: Math.random() * 100,
        y: Math.random() * 100,
      }))
      setSparkles(newSparkles)
      setTimeout(() => {
        setCelebrating(false)
        setSparkles([])
      }, 2000)
    }
    prevProfitRef.current = currentProfit
  }, [stats?.total_profit])

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
        <Loader size={32} className="animate-spin glow-text" />
        <span className="ml-3 uppercase tracking-wider font-mono" style={{ color: 'var(--theme-text-secondary)' }}>Loading statistics...</span>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Key Metrics with Visual Backgrounds */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 md:gap-6">
        <div className={`stat-card animate-fade-in-up relative overflow-hidden ${celebrating ? 'animate-celebrate profit-glow' : ''}`} style={{ animationDelay: '0.1s' }}>
          {/* Sparkle particles on profit celebration */}
          {sparkles.map(s => (
            <div key={s.id} className="sparkle-particle" style={{ left: `${s.x}%`, top: `${s.y}%` }} />
          ))}
          {/* Background image */}
          <div 
            className="absolute inset-0 opacity-10 blur-sm"
            style={{
              backgroundImage: 'url(https://images.unsplash.com/photo-1621761191319-c6fb62004040?w=400&h=300&fit=crop)',
              backgroundSize: 'cover',
              backgroundPosition: 'center',
            }}
          />
          <div className="relative z-10 flex items-center justify-between">
            <div className="flex-1 min-w-0">
              <p className="font-mono-tech text-[10px] sm:text-xs mb-2 sm:mb-3 uppercase tracking-widest flex items-center gap-2" style={{ color: 'var(--theme-text-secondary)' }}>
                <span className="icon-[tabler--coin] inline-block w-4 h-4"></span>
                Total Profit
                {celebrating && <Sparkles size={14} className="animate-spin" style={{ color: 'var(--theme-accent)' }} />}
              </p>
              <h3 className={`text-3xl sm:text-4xl md:text-5xl font-display font-black break-all ${celebrating ? 'animate-profit-number' : ''} ${(stats?.total_profit || 0) >= 0 ? 'glow-text' : ''}`} 
                  style={(stats?.total_profit || 0) >= 0 ? { color: 'var(--theme-success)' } : { color: 'var(--theme-error)', textShadow: '0 0 20px var(--theme-error)' }}>
                ◎{(stats?.total_profit || 0).toFixed(9)}
              </h3>
            </div>
            {(stats?.total_profit || 0) >= 0 ? (
              <div className={`p-4 rounded-2xl ${celebrating ? 'animate-celebrate' : 'animate-float'}`} style={{ 
                background: 'var(--glass-bg)',
                border: '2px solid var(--theme-success)',
                boxShadow: celebrating ? '0 0 50px var(--theme-success), 0 0 100px var(--theme-success)' : '0 0 30px var(--theme-success)'
              }}>
                <TrendingUp size={40} style={{ color: 'var(--theme-success)' }} />
              </div>
            ) : (
              <div className="p-4 rounded-2xl" style={{ 
                background: 'var(--glass-bg)',
                border: '2px solid var(--theme-error)',
                boxShadow: '0 0 30px var(--theme-error)'
              }}>
                <TrendingDown size={40} style={{ color: 'var(--theme-error)' }} />
              </div>
            )}
          </div>
        </div>

        <div className="stat-card animate-fade-in-up relative overflow-hidden" style={{ animationDelay: '0.2s' }}>
          {/* Background image */}
          <div 
            className="absolute inset-0 opacity-10 blur-sm"
            style={{
              backgroundImage: 'url(https://images.unsplash.com/photo-1642790106117-e829e14a795f?w=400&h=300&fit=crop)',
              backgroundSize: 'cover',
              backgroundPosition: 'center',
            }}
          />
          <div className="relative z-10 flex items-center justify-between">
            <div className="flex-1 min-w-0">
              <p className="font-mono-tech text-[10px] sm:text-xs mb-2 sm:mb-3 uppercase tracking-widest flex items-center gap-2" style={{ color: 'var(--theme-text-secondary)' }}>
                <span className="icon-[tabler--chart-line] inline-block w-4 h-4"></span>
                Total Trades
              </p>
              <h3 className="text-3xl sm:text-4xl md:text-5xl font-display font-black glow-text">
                {(stats?.total_buys || 0) + (stats?.total_sells || 0)}
              </h3>
            </div>
            <div className="p-4 rounded-2xl animate-float flex-shrink-0" style={{ 
              animationDelay: '0.5s',
              background: 'var(--glass-bg)',
              border: '2px solid var(--theme-accent)',
              boxShadow: '0 0 30px var(--glow-color)'
            }}>
              <Target size={40} style={{ color: 'var(--theme-accent)' }} />
            </div>
          </div>
        </div>

        <div className="stat-card animate-fade-in-up relative overflow-hidden" style={{ animationDelay: '0.3s' }}>
          {/* Background image */}
          <div 
            className="absolute inset-0 opacity-10 blur-sm"
            style={{
              backgroundImage: 'url(https://images.unsplash.com/photo-1639762681485-074b7f938ba0?w=400&h=300&fit=crop)',
              backgroundSize: 'cover',
              backgroundPosition: 'center',
            }}
          />
          <div className="relative z-10 flex items-center justify-between">
            <div className="flex-1 min-w-0">
              <p className="font-mono-tech text-[10px] sm:text-xs mb-2 sm:mb-3 uppercase tracking-widest flex items-center gap-2" style={{ color: 'var(--theme-text-secondary)' }}>
                <span className="icon-[tabler--wallet] inline-block w-4 h-4"></span>
                Active Holdings
              </p>
              <h3 className="text-3xl sm:text-4xl md:text-5xl font-display font-black" style={{ color: 'var(--theme-info)', textShadow: '0 0 20px var(--theme-info)' }}>
                {stats?.current_holdings?.length || 0}
              </h3>
              <div className="flex gap-3 mt-3">
                <span className="badge-success">BUYS: {stats?.total_buys || 0}</span>
                <span className="badge-error">SELLS: {stats?.total_sells || 0}</span>
              </div>
            </div>
            <div className="p-4 rounded-2xl animate-float flex-shrink-0" style={{ 
              animationDelay: '0.7s',
              background: 'var(--glass-bg)',
              border: '2px solid var(--theme-info)',
              boxShadow: '0 0 30px var(--theme-info)'
            }}>
              <Wallet size={40} style={{ color: 'var(--theme-info)' }} />
            </div>
          </div>
        </div>
        {/* New Coins widget */}
        <div className="stat-card animate-fade-in-up relative overflow-hidden" style={{ animationDelay: '0.4s' }}>
          <div className="relative z-10 flex items-center justify-between">
            <div className="flex-1 min-w-0">
              <p className="font-mono-tech text-[10px] sm:text-xs mb-2 sm:mb-3 uppercase tracking-widest" style={{ color: 'var(--theme-text-secondary)' }}>
                New Tokens Detected
              </p>
              <h3 className="text-3xl sm:text-4xl md:text-5xl font-display font-black glow-text">
                {totalDetectedCoins || detectedCoins?.length || 0}
              </h3>
              {detectedCoins && detectedCoins.length > 0 && (
                <p className="text-sm mt-3 text-gray-400 truncate">Latest: {detectedCoins[0].name || detectedCoins[0].symbol || detectedCoins[0].mint}</p>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* Charts and Trading Widget */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        <div className="glass-card p-6 rounded-2xl animate-slide-in-left lg:col-span-2">
          <h3 className="font-display text-lg font-black mb-5 glow-text uppercase tracking-wider">Profit Over Time</h3>
          <ResponsiveContainer width="100%" height={300}>
            <AreaChart data={chartData.length > 0 ? chartData : [{ time: 0, profit: 0, trades: 0 }]}>
              <defs>
                <linearGradient id="profitGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="var(--theme-accent)" stopOpacity={0.3}/>
                  <stop offset="95%" stopColor="var(--theme-accent)" stopOpacity={0.02}/>
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--theme-accent)" opacity={0.08} />
              <XAxis 
                dataKey="time" 
                stroke="var(--theme-text-secondary)"
                tick={{ fontSize: 11, fontFamily: 'JetBrains Mono' }}
              />
              <YAxis stroke="var(--theme-text-secondary)" tick={{ fontSize: 11, fontFamily: 'JetBrains Mono' }} />
              <Tooltip 
                contentStyle={{ 
                  backgroundColor: 'var(--theme-bg-card)', 
                  border: '2px solid var(--theme-accent)', 
                  borderRadius: '0.75rem',
                  backdropFilter: 'blur(12px)',
                  fontFamily: 'JetBrains Mono',
                  fontSize: '12px',
                  boxShadow: '0 0 20px var(--glow-color)'
                }}
                cursor={{ stroke: 'var(--theme-accent)', strokeDasharray: '4 4' }}
                formatter={(value: any) => [value.toFixed(9) + ' SOL', 'Profit']}
                labelFormatter={(label) => `Point ${label}`}
              />
              <Area 
                type="monotone" 
                dataKey="profit" 
                stroke="var(--theme-accent)" 
                strokeWidth={2.5} 
                fill="url(#profitGradient)"
                dot={false}
                activeDot={{ r: 6, fill: 'var(--theme-accent)', stroke: 'var(--theme-bg-card)', strokeWidth: 2, style: { filter: 'drop-shadow(0 0 8px var(--theme-accent))' } }}
                isAnimationActive={true}
                animationDuration={1500}
                animationEasing="ease-in-out"
              />
            </AreaChart>
          </ResponsiveContainer>
        </div>

        {/* Trading Performance Widget */}
        <div className="animate-slide-in-right">
          <TradingPerformanceWidget />
        </div>
      </div>

      {/* Trade Activity Chart - Full Width */}
      <div className="glass-card p-6 rounded-2xl animate-fade-in-up">
        <h3 className="font-display text-lg font-black mb-5 glow-text uppercase tracking-wider">Trade Activity</h3>
          <ResponsiveContainer width="100%" height={300}>
            <BarChart data={chartData.length > 0 ? chartData : [{ time: 0, profit: 0, trades: 0 }]}>
              <defs>
                <linearGradient id="barGradient" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="0%" stopColor="var(--theme-accent)" stopOpacity={0.9}/>
                  <stop offset="100%" stopColor="var(--theme-accent)" stopOpacity={0.3}/>
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="var(--theme-accent)" opacity={0.08} />
              <XAxis 
                dataKey="time" 
                stroke="var(--theme-text-secondary)"
                tick={{ fontSize: 11, fontFamily: 'JetBrains Mono' }}
              />
              <YAxis stroke="var(--theme-text-secondary)" tick={{ fontSize: 11, fontFamily: 'JetBrains Mono' }} />
              <Tooltip 
                contentStyle={{ 
                  backgroundColor: 'var(--theme-bg-card)', 
                  border: '2px solid var(--theme-accent)', 
                  borderRadius: '0.75rem',
                  backdropFilter: 'blur(12px)',
                  fontFamily: 'JetBrains Mono',
                  fontSize: '12px',
                  boxShadow: '0 0 20px var(--glow-color)'
                }}
                labelFormatter={(label) => `Point ${label}`}
              />
              <Bar 
                dataKey="trades" 
                fill="url(#barGradient)" 
                radius={[8, 8, 0, 0]} 
                isAnimationActive={true}
                animationDuration={1200}
                animationEasing="ease-out"
              />
            </BarChart>
          </ResponsiveContainer>
      </div>

      {/* Status Info */}
      <div className="cyber-card p-6">
        <h3 className="font-display text-lg font-black mb-5 glow-text uppercase tracking-wider">Bot Information</h3>
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 font-mono-tech">
          <div className="p-4 bg-black electric-border group hover:scale-105 transition-transform animate-glow-breathe">
            <p className="text-[var(--theme-text-secondary)] text-[10px] mb-3 uppercase tracking-widest font-mono-tech">Buys Executed</p>
            <p className="text-3xl font-black glow-text font-display">{stats?.total_buys || 0}</p>
          </div>
          <div className="p-4 bg-black electric-border group hover:scale-105 transition-transform animate-glow-breathe" style={{ animationDelay: '1s' }}>
            <p className="text-[var(--theme-text-secondary)] text-[10px] mb-3 uppercase tracking-widest font-mono-tech">Sells Executed</p>
            <p className="text-3xl font-black glow-text font-display">{stats?.total_sells || 0}</p>
          </div>
          <div className="p-4 bg-black electric-border group hover:scale-105 transition-transform animate-glow-breathe" style={{ animationDelay: '2s' }}>
            <p className="text-[var(--theme-text-secondary)] text-[10px] mb-3 uppercase tracking-widest font-mono-tech">Win Rate</p>
            <p className="text-3xl font-black glow-text font-display">
              {stats?.total_buys && stats.total_buys > 0 ? Math.round((stats.total_sells / stats.total_buys) * 100) : 0}%
            </p>
          </div>
          <div className="p-4 bg-black electric-border group hover:scale-105 transition-transform animate-glow-breathe" style={{ animationDelay: '3s' }}>
            <p className="text-[var(--theme-text-secondary)] text-[10px] mb-3 uppercase tracking-widest font-mono-tech">Uptime</p>
            <p className="text-3xl font-black glow-text font-display">
              {stats?.uptime_secs ? (stats.uptime_secs / 3600).toFixed(1) : 0}h
            </p>
          </div>
        </div>
      </div>
    </div>
  )
}
