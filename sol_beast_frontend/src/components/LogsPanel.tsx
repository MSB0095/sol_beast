import { useBotStore, LogEntry } from '../store/botStore'
import { AlertCircle, Info, AlertTriangle, Trash2, Download } from 'lucide-react'
import { useState } from 'react'

export default function LogsPanel() {
  const { logs, clearLogs } = useBotStore()
  const [filter, setFilter] = useState<'all' | 'info' | 'warn' | 'error'>('all')

  const filteredLogs = filter === 'all' 
    ? logs 
    : logs.filter(log => log.level === filter)

  const getLogIcon = (level: LogEntry['level']) => {
    switch (level) {
      case 'info':
        return <Info size={16} className="text-blue-400" />
      case 'warn':
        return <AlertTriangle size={16} className="text-yellow-400" />
      case 'error':
        return <AlertCircle size={16} className="text-red-400" />
    }
  }

  const getLogColor = (level: LogEntry['level']) => {
    switch (level) {
      case 'info':
        return 'border-blue-500/20 bg-blue-900/10'
      case 'warn':
        return 'border-yellow-500/20 bg-yellow-900/10'
      case 'error':
        return 'border-red-500/20 bg-red-900/10'
    }
  }

  const exportLogs = () => {
    const logText = filteredLogs.map(log => 
      `[${new Date(log.timestamp).toLocaleString()}] [${log.level.toUpperCase()}] ${log.message}${log.details ? '\n  Details: ' + log.details : ''}`
    ).join('\n\n')
    
    const blob = new Blob([logText], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `sol-beast-logs-${new Date().toISOString()}.txt`
    document.body.appendChild(a)
    a.click()
    document.body.removeChild(a)
    URL.revokeObjectURL(url)
  }

  const formatTimestamp = (timestamp: string) => {
    const date = new Date(timestamp)
    return date.toLocaleTimeString('en-US', { 
      hour: '2-digit', 
      minute: '2-digit', 
      second: '2-digit',
      hour12: false 
    })
  }

  return (
    <div className="space-y-4">
      {/* Header with controls */}
      <div className="card-enhanced rounded-xl p-4">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold gradient-text">Bot Logs</h3>
          <div className="flex gap-2">
            <button
              onClick={exportLogs}
              disabled={filteredLogs.length === 0}
              className="px-3 py-1.5 bg-sol-purple/20 hover:bg-sol-purple/30 border border-sol-purple/50 rounded-lg text-sm flex items-center gap-2 transition-all disabled:opacity-50 disabled:cursor-not-allowed hover:scale-105"
            >
              <Download size={14} />
              Export
            </button>
            <button
              onClick={clearLogs}
              disabled={logs.length === 0}
              className="px-3 py-1.5 bg-red-500/20 hover:bg-red-500/30 border border-red-500/50 rounded-lg text-sm flex items-center gap-2 transition-all disabled:opacity-50 disabled:cursor-not-allowed hover:scale-105"
            >
              <Trash2 size={14} />
              Clear
            </button>
          </div>
        </div>

        {/* Filter buttons */}
        <div className="flex gap-2">
          {(['all', 'info', 'warn', 'error'] as const).map((level) => (
            <button
              key={level}
              onClick={() => setFilter(level)}
              className={`px-3 py-1.5 rounded-lg text-sm font-medium transition-all ${
                filter === level
                  ? 'bg-gradient-to-r from-sol-purple to-sol-cyan text-white shadow-glow'
                  : 'bg-gray-700 text-gray-300 hover:bg-gray-600'
              }`}
            >
              {level === 'all' ? 'All' : level.charAt(0).toUpperCase() + level.slice(1)}
              {level !== 'all' && (
                <span className="ml-1.5 text-xs opacity-70">
                  ({logs.filter(l => l.level === level).length})
                </span>
              )}
            </button>
          ))}
        </div>
      </div>

      {/* Logs display */}
      <div className="card-enhanced rounded-xl p-4 max-h-[600px] overflow-y-auto">
        {filteredLogs.length === 0 ? (
          <div className="text-center py-12 text-gray-400">
            <Info size={48} className="mx-auto mb-3 opacity-30" />
            <p>No logs to display</p>
            <p className="text-sm text-gray-500 mt-1">
              {filter !== 'all' ? `No ${filter} messages` : 'Logs will appear here as the bot runs'}
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {filteredLogs.map((log, index) => (
              <div
                key={index}
                className={`border rounded-lg p-3 ${getLogColor(log.level)} transition-all hover:border-opacity-50`}
              >
                <div className="flex items-start gap-3">
                  <div className="flex-shrink-0 mt-0.5">
                    {getLogIcon(log.level)}
                  </div>
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1">
                      <span className="text-xs font-mono text-gray-400">
                        {formatTimestamp(log.timestamp)}
                      </span>
                      <span className={`text-xs font-semibold uppercase px-1.5 py-0.5 rounded ${
                        log.level === 'info' ? 'bg-blue-500/20 text-blue-300' :
                        log.level === 'warn' ? 'bg-yellow-500/20 text-yellow-300' :
                        'bg-red-500/20 text-red-300'
                      }`}>
                        {log.level}
                      </span>
                    </div>
                    <p className="text-sm text-gray-200 break-words">{log.message}</p>
                    {log.details && (
                      <details className="mt-2">
                        <summary className="text-xs text-gray-400 cursor-pointer hover:text-gray-300">
                          Show details
                        </summary>
                        <pre className="mt-2 text-xs text-gray-300 bg-black/30 p-2 rounded overflow-x-auto">
                          {log.details}
                        </pre>
                      </details>
                    )}
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Stats summary */}
      <div className="grid grid-cols-3 gap-4">
        <div className="card-enhanced rounded-xl border-blue-500/20 p-4 hover:scale-105">
          <div className="flex items-center gap-2 mb-2">
            <Info size={16} className="text-blue-400" />
            <span className="text-sm text-gray-400 font-medium">Info</span>
          </div>
          <p className="text-2xl font-bold text-blue-400">
            {logs.filter(l => l.level === 'info').length}
          </p>
        </div>
        
        <div className="card-enhanced rounded-xl border-yellow-500/20 p-4 hover:scale-105">
          <div className="flex items-center gap-2 mb-2">
            <AlertTriangle size={16} className="text-yellow-400" />
            <span className="text-sm text-gray-400 font-medium">Warnings</span>
          </div>
          <p className="text-2xl font-bold text-yellow-400">
            {logs.filter(l => l.level === 'warn').length}
          </p>
        </div>
        
        <div className="card-enhanced rounded-xl border-red-500/20 p-4 hover:scale-105">
          <div className="flex items-center gap-2 mb-2">
            <AlertCircle size={16} className="text-red-400" />
            <span className="text-sm text-gray-400 font-medium">Errors</span>
          </div>
          <p className="text-2xl font-bold text-red-400">
            {logs.filter(l => l.level === 'error').length}
          </p>
        </div>
      </div>
    </div>
  )
}
