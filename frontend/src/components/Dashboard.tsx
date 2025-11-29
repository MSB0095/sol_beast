import { useMemo, useState } from 'react'
import { useBotStore } from '../store/botStore'
import { LineChart, Line, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer, BarChart, Bar, AreaChart, Area, PieChart, Pie, Cell } from 'recharts'
import { TrendingUp, TrendingDown, Target, Loader, Wallet, BarChart3, Activity, Zap, DollarSign, Clock, Shield, AlertCircle, CheckCircle, ArrowUpRight, ArrowDownRight } from 'lucide-react'
import TradingPerformanceWidget from './TradingPerformanceWidget'

export default function Dashboard() {
  const { stats, historicalData, startBot, stopBot, runningState, setMode } = useBotStore()
  const [activeChart, setActiveChart] = useState<'line' | 'area' | 'bar'>('line')
  const [timeRange, setTimeRange] = useState<'1h' | '6h' | '24h' | '7d'>('6h')

  // Generate chart data from historical data
  const chartData = useMemo(() => {
    if (historicalData.length === 0) {
      return []
    }
    
    return historicalData.map((point, index) => ({
      time: index,
      profit: point.profit,
      trades: point.trades,
      volume: point.holdings || 0,
      timestamp: point.timestamp,
      formattedTime: new Date(point.timestamp).toLocaleTimeString(),
    }))
  }, [historicalData])

  // Statistics items with enhanced data
  const statisticsItems = useMemo(() => {
    if (!stats) return []
    
    const totalTrades = (stats.total_buys || 0) + (stats.total_sells || 0)
    const winRate = stats.total_buys > 0 ? Math.round((stats.total_sells / stats.total_buys) * 100) : 0
    const currentValue = stats.current_holdings?.length || 0
    const avgTrade = totalTrades > 0 ? stats.total_profit / totalTrades : 0
    // Derive a simple change metric using the last two historical points if available
    let profitChangePercent = 0
    if (historicalData.length >= 2) {
      const last = historicalData[historicalData.length - 1].profit
      const prev = historicalData[historicalData.length - 2].profit
      if (prev !== 0) profitChangePercent = ((last - prev) / Math.abs(prev)) * 100
    }

    const formattedProfitChange = `${profitChangePercent >= 0 ? '+' : ''}${profitChangePercent.toFixed(2)}%`
    const profitPositive = (stats?.total_profit || 0) >= 0

    return [
      {
        icon: profitPositive ? TrendingUp : TrendingDown,
        title: 'Total Profit',
        value: `◎${(stats?.total_profit || 0).toFixed(6)}`,
        change: formattedProfitChange,
        changeType: profitChangePercent >= 0 ? 'positive' : 'negative',
        status: profitPositive ? 'success' : 'error',
        color: profitPositive ? 'var(--theme-success)' : 'var(--theme-error)',
        description: 'Total profit/loss'
      },
      {
        icon: Target,
        title: 'Total Trades',
        value: totalTrades.toString(),
        change: totalTrades > 0 ? `${Math.round((totalTrades / Math.max(1, (historicalData.length || 1))) * 100)}%` : '0%',
        changeType: 'positive',
        status: 'info',
        color: 'var(--theme-accent)',
        description: `${winRate}% win rate`
      },
      {
        icon: Wallet,
        title: 'Active Holdings',
        value: currentValue.toString(),
        change: currentValue > 0 ? `${Math.round((currentValue / Math.max(1, (stats.current_holdings?.length || 1))) * 100)}%` : '0%',
        changeType: currentValue > 0 ? 'positive' : 'negative',
        status: 'warning',
        color: 'var(--theme-info)',
        description: 'Current positions'
      },
      {
        icon: DollarSign,
        title: 'Avg Trade',
        value: `◎${avgTrade.toFixed(8)}`,
        change: avgTrade !== 0 ? `${(avgTrade / Math.max(1, avgTrade) * 100).toFixed(2)}%` : '0%',
        changeType: 'positive',
        status: 'primary',
        color: 'var(--theme-warning)',
        description: 'Per transaction'
      }
    ]
  }, [stats, historicalData])

  // Performance distribution data
  const performanceData = useMemo(() => {
    if (!stats) return []
    const successful = stats.total_sells || 0
    const pending = Math.max(0, (stats.total_buys || 0) - (stats.total_sells || 0))
    const failed = 0

    return [
      { name: 'Successful', value: successful, color: 'var(--theme-success)' },
      { name: 'Pending', value: pending, color: 'var(--theme-warning)' },
      { name: 'Failed', value: failed, color: 'var(--theme-error)' }
    ]
  }, [stats])

  // Hourly performance data
  const hourlyData = useMemo(() => {
    // Build hourly series from historicalData (last CHART_HISTORY_POINTS)
    if (historicalData.length === 0) return []
    const points = historicalData.slice(-24)
    return points.map((p) => ({
      hour: new Date(p.timestamp).getHours(),
      trades: p.trades,
      volume: p.holdings || 0,
      profit: p.profit,
      timestamp: p.timestamp
    }))
  }, [historicalData])

  if (!stats) {
    return (
      <div className="flex items-center justify-center min-h-[400px]">
        <div className="text-center">
          <Loader size={48} className="animate-spin text-primary mx-auto mb-4" />
          <p className="text-primary font-mono uppercase tracking-wider">Initializing trading dashboard...</p>
          <div className="mt-4 w-64 h-2 bg-base-300 rounded-full overflow-hidden mx-auto">
            <div className="h-full bg-primary rounded-full" style={{ width: '60%' }} />
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-8 animate-fade-in-up">
      {/* Header Section */}
      <div className="flex flex-col lg:flex-row lg:items-center lg:justify-between gap-4">
        <div>
          <h1 className="text-3xl lg:text-4xl font-black text-primary uppercase tracking-wider flex items-center gap-3">
            <Activity className="w-8 h-8 animate-buzz" />
            Trading Command Center
          </h1>
          <p className="text-base-content/60 font-mono mt-2">Real-time monitoring and analytics</p>
        </div>
        
        {/* Chart Controls */}
        <div className="flex items-center gap-4">
          <div className="join">
            {(['1h', '6h', '24h', '7d'] as const).map((range) => (
              <button
                key={range}
                onClick={() => setTimeRange(range)}
                className={`btn btn-sm join-item ${timeRange === range ? 'btn-active' : 'btn-ghost'}`}
              >
                {range}
              </button>
            ))}
          </div>
          
          <div className="join">
            {(['line', 'area', 'bar'] as const).map((chart) => (
              <button
                key={chart}
                onClick={() => setActiveChart(chart)}
                className={`btn btn-sm join-item capitalize ${activeChart === chart ? 'btn-active' : 'btn-ghost'}`}
              >
                {chart}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Main Statistics Cards */}
      <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-4 gap-6">
        {statisticsItems.map((item, index) => {
          const IconComponent = item.icon
          const isPositive = item.changeType === 'positive'
          
          return (
            <div
              key={item.title}
              className="card glass-card bg-base-100/80 border border-primary/10 transition-all duration-300 group"
              style={{
                animationDelay: `${index * 0.1}s`,
                boxShadow: isPositive ? `0 0 30px rgba(34, 197, 94, 0.3)` : `0 0 30px rgba(239, 68, 68, 0.3)`
              }}
            >
              <div className="card-body p-6">
                <div className="flex items-center justify-between mb-4">
                  <div className="flex items-center gap-3">
                      <div className="p-3 rounded-xl bg-primary/10 group-hover:bg-primary/20 transition-colors animate-fuzz">
                      <IconComponent size={24} style={{ color: item.color }} />
                    </div>
                    <div>
                      <div className="text-base-content/60 text-sm font-mono uppercase tracking-wider">
                        {item.title}
                      </div>
                      <div className="text-xs text-base-content/40">{item.description}</div>
                    </div>
                  </div>
                  
                  <div className="text-right">
                    <div className="flex items-center gap-1 text-xs">
                      {isPositive ? <ArrowUpRight size={14} className="text-success" /> : <ArrowDownRight size={14} className="text-error" />}
                      <span className={isPositive ? 'text-success' : 'text-error'}>{item.change}</span>
                    </div>
                  </div>
                </div>
                
                <div className="space-y-2">
                  <div
                    className={`text-3xl font-black`}
                    style={{
                      color: item.color,
                      textShadow: item.status === 'error' ? '0 0 6px var(--theme-error)' : undefined
                    }}
                  >
                    {item.value}
                  </div>
                  
                  {/* Progress indicator */}
                  <div className="w-full h-1 bg-base-300 rounded-full overflow-hidden">
                    <div
                      className="h-full rounded-full transition-all duration-1000"
                      style={{
                        width: `${Math.min(100, Math.abs(parseFloat(item.change)) * 20)}%`,
                        background: `linear-gradient(90deg, ${item.color}, ${item.color}aa)`
                      }}
                    />
                  </div>
                </div>
              </div>
            </div>
          )
        })}
      </div>

      {/* Charts Section */}
      <div className="grid grid-cols-1 xl:grid-cols-3 gap-8">
        {/* Main Chart */}
        <div className="xl:col-span-2 space-y-6">
          {/* Profit Chart */}
          <div className="card glass-card bg-base-100/80 border border-primary/10">
            <div className="card-body">
              <div className="flex items-center justify-between mb-6">
                <h3 className="card-title text-primary flex items-center gap-3">
                  <BarChart3 className="w-6 h-6" />
                  Profit Analytics
                </h3>
                <div className="badge badge-outline">{timeRange}</div>
              </div>
              
              <div className="h-80">
                <ResponsiveContainer width="100%" height="100%">
                  {activeChart === 'line' ? (
                    <LineChart data={chartData.length > 0 ? chartData : [{ time: 0, profit: 0 }]}> 
                      <CartesianGrid strokeDasharray="3 3" stroke="var(--theme-accent)" opacity={0.1} />
                      <XAxis dataKey="time" stroke="var(--theme-text-secondary)" />
                      <YAxis stroke="var(--theme-text-secondary)" />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'var(--theme-bg-secondary)',
                          border: '1px solid var(--theme-accent)',
                          borderRadius: '12px',
                          backdropFilter: 'blur(16px)'
                        }}
                        formatter={(value: unknown) => {
                          if (typeof value === 'number') return [`◎${value.toFixed(8)}`, 'Profit']
                          return [String(value), 'Profit']
                        }}
                      />
                      <Line type="monotone" dataKey="profit" stroke="var(--theme-accent)" strokeWidth={3} dot={false} />
                    </LineChart>
                  ) : activeChart === 'area' ? (
                    <AreaChart data={chartData.length > 0 ? chartData : [{ time: 0, profit: 0 }]}> 
                      <CartesianGrid strokeDasharray="3 3" stroke="var(--theme-accent)" opacity={0.1} />
                      <XAxis dataKey="time" stroke="var(--theme-text-secondary)" />
                      <YAxis stroke="var(--theme-text-secondary)" />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'var(--theme-bg-secondary)',
                          border: '1px solid var(--theme-accent)',
                          borderRadius: '12px',
                          backdropFilter: 'blur(16px)'
                        }}
                        formatter={(value: unknown) => {
                          if (typeof value === 'number') return [`◎${value.toFixed(8)}`, 'Profit']
                          return [String(value), 'Profit']
                        }}
                      />
                      <Area type="monotone" dataKey="profit" stroke="var(--theme-accent)" fill="var(--theme-accent)" fillOpacity={0.2} />
                    </AreaChart>
                  ) : (
                    <BarChart data={chartData.length > 0 ? chartData : [{ time: 0, trades: 0 }]}> 
                      <CartesianGrid strokeDasharray="3 3" stroke="var(--theme-accent)" opacity={0.1} />
                      <XAxis dataKey="time" stroke="var(--theme-text-secondary)" />
                      <YAxis stroke="var(--theme-text-secondary)" />
                      <Tooltip
                        contentStyle={{
                          backgroundColor: 'var(--theme-bg-secondary)',
                          border: '1px solid var(--theme-accent)',
                          borderRadius: '12px',
                          backdropFilter: 'blur(16px)'
                        }}
                        formatter={(value: unknown) => [typeof value === 'number' ? value : String(value), 'Trades']}
                      />
                      <Bar dataKey="trades" fill="var(--theme-accent)" radius={[4, 4, 0, 0]} />
                    </BarChart>
                  )}
                </ResponsiveContainer>
              </div>
            </div>
          </div>

          {/* Hourly Performance */}
          <div className="card glass-card bg-base-100/80 border border-primary/10">
            <div className="card-body">
              <h3 className="card-title text-primary flex items-center gap-3 mb-6">
                <Clock className="w-6 h-6" />
                Hourly Performance
              </h3>
              
              <div className="h-64">
                <ResponsiveContainer width="100%" height="100%">
                  <AreaChart data={hourlyData}>
                    <CartesianGrid strokeDasharray="3 3" stroke="var(--theme-accent)" opacity={0.1} />
                    <XAxis dataKey="hour" stroke="var(--theme-text-secondary)" tickFormatter={(hour) => `${hour}:00`} />
                    <YAxis stroke="var(--theme-text-secondary)" />
                    <Tooltip
                      contentStyle={{
                        backgroundColor: 'var(--theme-bg-secondary)',
                        border: '1px solid var(--theme-accent)',
                        borderRadius: '12px',
                        backdropFilter: 'blur(16px)'
                      }}
                      labelFormatter={(hour) => `${hour}:00 - ${hour + 1}:00`}
                    />
                    <Area type="monotone" dataKey="volume" stroke="var(--theme-info)" fill="var(--theme-info)" fillOpacity={0.2} />
                  </AreaChart>
                </ResponsiveContainer>
              </div>
            </div>
          </div>
        </div>

        {/* Side Panel */}
        <div className="space-y-6">
          {/* Performance Distribution */}
          <div className="card glass-card bg-base-100/80 border border-primary/10">
            <div className="card-body">
              <h3 className="card-title text-primary flex items-center gap-3 mb-6">
                <Target className="w-6 h-6" />
                Trade Distribution
              </h3>
              
              <div className="h-48">
                <ResponsiveContainer width="100%" height="100%">
                  <PieChart>
                    <Pie
                      data={performanceData}
                      cx="50%"
                      cy="50%"
                      innerRadius={40}
                      outerRadius={80}
                      dataKey="value"
                    >
                      {performanceData.map((entry, index) => (
                        <Cell key={`cell-${index}`} fill={entry.color} />
                      ))}
                    </Pie>
                    <Tooltip />
                  </PieChart>
                </ResponsiveContainer>
              </div>
              
              <div className="space-y-2 mt-4">
                {performanceData.map((item, index) => (
                  <div key={index} className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <div className="w-3 h-3 rounded-full" style={{ backgroundColor: item.color }} />
                      <span className="text-sm">{item.name}</span>
                    </div>
                    <span className="text-sm font-mono">{item.value}</span>
                  </div>
                ))}
              </div>
            </div>
          </div>

          {/* Quick Actions */}
          <div className="card glass-card bg-base-100/80 border border-primary/10">
            <div className="card-body">
              <h3 className="card-title text-primary flex items-center gap-3 mb-6">
                <Zap className="w-6 h-6" />
                Quick Actions
              </h3>
              
              <div className="space-y-3">
                <button
                  onClick={async () => {
                    try {
                      await startBot()
                    } catch (err) {
                      console.error('Start bot failed', err)
                    }
                  }}
                  className="btn btn-primary w-full justify-start gap-3"
                  disabled={runningState === 'starting' || runningState === 'running'}
                >
                  <CheckCircle size={18} />
                  Start Trading
                </button>
                <button
                  onClick={async () => {
                    try {
                      await stopBot()
                    } catch (err) {
                      console.error('Stop bot failed', err)
                    }
                  }}
                  className="btn btn-warning w-full justify-start gap-3"
                  disabled={runningState === 'stopping' || runningState === 'stopped'}
                >
                  <AlertCircle size={18} />
                  Emergency Stop
                </button>
                <button
                  onClick={async () => {
                    try {
                      // switch to dry-run for review
                      await setMode('dry-run')
                    } catch (err) {
                      console.error('Failed to set mode:', err)
                    }
                  }}
                  className="btn btn-info w-full justify-start gap-3"
                >
                  <Shield size={18} />
                  Risk Review
                </button>
              </div>
            </div>
          </div>

          {/* System Status */}
          <div className="card glass-card bg-base-100/80 border border-primary/10">
            <div className="card-body">
              <h3 className="card-title text-primary flex items-center gap-3 mb-6">
                <Shield className="w-6 h-6" />
                System Status
              </h3>
              
              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <span className="text-sm">Connection</span>
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 bg-success rounded-full animate-pulse" />
                    <span className="text-success text-sm">Online</span>
                  </div>
                </div>
                
                <div className="flex items-center justify-between">
                  <span className="text-sm">Bot Status</span>
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 bg-success rounded-full animate-pulse" />
                    <span className="text-success text-sm">Active</span>
                  </div>
                </div>
                
                <div className="flex items-center justify-between">
                  <span className="text-sm">Risk Level</span>
                  <div className="flex items-center gap-2">
                    <div className="w-2 h-2 bg-warning rounded-full" />
                    <span className="text-warning text-sm">Medium</span>
                  </div>
                </div>
                
                <div className="flex items-center justify-between">
                  <span className="text-sm">Uptime</span>
                  <span className="text-primary text-sm font-mono">
                    {stats?.uptime_secs ? (stats.uptime_secs / 3600).toFixed(1) : '0.0'}h
                  </span>
                </div>
              </div>
            </div>
          </div>

          {/* Trading Performance Widget */}
          <div className="animate-slide-in-right">
            <TradingPerformanceWidget />
          </div>
        </div>
      </div>
    </div>
  )
}
