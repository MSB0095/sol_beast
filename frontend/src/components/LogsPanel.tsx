import { useBotStore, LogEntry } from '../store/botStore'
import { useWasmStore } from '../store/wasmStore'
import { AlertCircle, Info, AlertTriangle, Trash2, Download, FileText } from 'lucide-react'
import { useState, useEffect } from 'react'
import { RUNTIME_MODE } from '../config'

export default function LogsPanel() {
  const { logs: apiLogs, clearLogs } = useBotStore()
  const { initialized: wasmInitialized, initializeWasm, getLogs: getWasmLogs, clearLogs: clearWasmLogs, error: wasmError } = useWasmStore()
  const [wasmLogs, setWasmLogs] = useState<LogEntry[]>([])
  const [logMode, setLogMode] = useState<'auto' | 'wasm' | 'api'>('auto')
  const [fallbackToApi, setFallbackToApi] = useState(false)
  const effectiveSource = logMode === 'auto'
    ? (RUNTIME_MODE === 'frontend-wasm' ? (fallbackToApi ? 'api' : 'wasm') : 'api')
    : logMode
  
  useEffect(() => {
    let intervalId: number | undefined
    let fallbackTimer: number | undefined

    const setup = async () => {
      if (effectiveSource !== 'wasm') return

      try {
        if (!wasmInitialized) {
          await initializeWasm()
        }

        const fetchWasmLogs = async () => {
          try {
            const logs = await getWasmLogs()
            if (Array.isArray(logs)) {
              setWasmLogs(logs as unknown as LogEntry[])
            }
          } catch (err) {
            console.error('Failed to fetch WASM logs:', err)
          }
        }

        // fetch immediately and then poll
        await fetchWasmLogs()
        intervalId = window.setInterval(fetchWasmLogs, 2000)

        // If wasm logs remain empty but API logs exist after 3s, fallback to API
        fallbackTimer = window.setTimeout(() => {
          if ((wasmLogs.length === 0 || !wasmInitialized) && apiLogs.length > 0) {
            setFallbackToApi(true)
          }
        }, 3000)
      } catch (err) {
        console.error('WASM logs poll setup failed:', err)
        // fallback to API logs immediately if available
        if (apiLogs.length > 0) setFallbackToApi(true)
      }
    }

    setup()

    return () => {
      if (intervalId) {
        clearInterval(intervalId)
      }
      if (fallbackTimer) {
        clearTimeout(fallbackTimer)
      }
    }
  }, [wasmInitialized, initializeWasm, getWasmLogs, effectiveSource, apiLogs, wasmLogs])

  const [filter, setFilter] = useState<'all' | 'info' | 'warn' | 'error'>('all')

  const displayLogs = effectiveSource === 'wasm' ? wasmLogs : apiLogs

  const sourceBadge = effectiveSource === 'wasm' ? 'WASM' : 'API'

  const filteredLogs = filter === 'all'
    ? displayLogs
    : displayLogs.filter(log => log.level === filter)

  const getLogIcon = (level: LogEntry['level']) => {
    switch (level) {
      case 'info':
        return <Info className="w-5 h-5 text-info" />
      case 'warn':
        return <AlertTriangle className="w-5 h-5 text-warning" />
      case 'error':
        return <AlertCircle className="w-5 h-5 text-error" />
    }
  }

  const getLogBadgeColor = (level: LogEntry['level']) => {
    switch (level) {
      case 'info':
        return 'badge-info'
      case 'warn':
        return 'badge-warning'
      case 'error':
        return 'badge-error'
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
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="p-3 bg-secondary/10 rounded-lg">
          <FileText className="w-6 h-6 text-secondary" />
        </div>
        <div>
          <h3 className="text-xl font-bold text-base-content uppercase tracking-wider">
            Bot Logs
          </h3>
          <p className="text-base-content/60">
            System activity and debugging information
          </p>
        </div>
      </div>

      {/* Controls */}
      <div className="card bg-base-200/50 border border-base-300 rounded-xl">
        <div className="card-body">
          <div className="flex flex-col lg:flex-row gap-4 items-start lg:items-center justify-between">
            {/* Source Selector */}
            <div className="flex items-center gap-3">
              <span className="text-sm font-medium text-base-content/60 uppercase">Log Source:</span>
              <div className="join">
                {(['auto', 'api', 'wasm'] as const).map((mode) => (
                  <button
                    key={mode}
                    onClick={() => setLogMode(mode)}
                    className={`join-item btn btn-sm ${logMode === mode ? 'btn-primary' : 'btn-soft btn-ghost'}`}
                  >
                    {mode.toUpperCase()}
                  </button>
                ))}
              </div>
              <span className="badge badge-sm">
                {effectiveSource === 'wasm' ? 'WASM' : 'API'}
              </span>
            </div>

            {/* Actions */}
            <div className="flex gap-2">
              <button
                onClick={exportLogs}
                disabled={filteredLogs.length === 0}
                className="btn btn-soft btn-sm gap-2"
              >
                <Download className="w-4 h-4" />
                Export
              </button>
              <button
                onClick={async () => {
                  try {
                    if (effectiveSource === 'wasm') {
                      await clearWasmLogs()
                      setWasmLogs([])
                    } else {
                      clearLogs()
                    }
                  } catch (err) {
                    console.error('Clear logs failed', err)
                  }
                }}
                disabled={displayLogs.length === 0}
                className="btn btn-soft btn-sm gap-2 btn-error"
              >
                <Trash2 className="w-4 h-4" />
                Clear
              </button>
            </div>
          </div>

          {/* Filter buttons */}
          <div className="flex flex-wrap gap-2 mt-4">
            {(['all', 'info', 'warn', 'error'] as const).map((level) => (
              <button
                key={level}
                onClick={() => setFilter(level)}
                className={`btn btn-sm ${filter === level ? 'btn-primary' : 'btn-soft btn-ghost'}`}
              >
                {level === 'all' ? 'All' : level.charAt(0).toUpperCase() + level.slice(1)}
                {level !== 'all' && (
                  <span className="badge badge-sm ml-2">
                    {displayLogs.filter(l => l.level === level).length}
                  </span>
                )}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* WASM Error Alert */}
      {wasmError && effectiveSource === 'wasm' && (
        <div role="alert" className="alert alert-error">
          <AlertCircle className="w-5 h-5" />
          <div>
            <h3 className="font-bold uppercase tracking-wider">WASM Error</h3>
            <div className="text-xs">{wasmError}</div>
          </div>
          <div>
            <button
              onClick={() => initializeWasm()}
              className="btn btn-sm btn-ghost"
            >
              Retry
            </button>
          </div>
        </div>
      )}

      {/* Active Source Badge */}
      <div className="mb-2">
        <span className="badge badge-sm">Source: {sourceBadge}{fallbackToApi ? ' (fallback)' : ''}</span>
      </div>

      {/* Logs List */}
      <div className="card bg-base-200/50 border border-base-300 rounded-xl">
        <div className="card-body">
          {filteredLogs.length === 0 ? (
            <div className="text-center py-12">
              <div className="flex flex-col items-center gap-4">
                <div className="p-4 bg-base-100 rounded-full">
                  <FileText className="w-12 h-12 text-base-content/50" />
                </div>
                <div>
                  <h4 className="text-lg font-semibold text-base-content mb-2">No logs to display</h4>
                  <p className="text-base-content/60">
                    {filter !== 'all' ? `No ${filter} messages` : 'Logs will appear here as the bot runs'}
                  </p>
                </div>
              </div>
            </div>
          ) : (
            <div className="space-y-3 max-h-[500px] overflow-y-auto">
              {filteredLogs.map((log, index) => (
                <div
                  key={index}
                  className="card bg-base-100 border border-base-300 hover:border-base-content/20 transition-all"
                >
                  <div className="card-body p-4">
                    <div className="flex items-start gap-3">
                      <div className="flex-shrink-0 mt-1">
                        {getLogIcon(log.level)}
                      </div>
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-2">
                          <span className="text-xs font-mono text-base-content/60">
                            {formatTimestamp(log.timestamp)}
                          </span>
                          <span className={`badge badge-sm ${getLogBadgeColor(log.level)}`}>
                            {log.level.toUpperCase()}
                          </span>
                        </div>
                        <p className="text-sm text-base-content break-words">{log.message}</p>
                        {log.details && (
                          <div className="mt-3">
                            <details className="collapse collapse-arrow">
                              <summary className="collapse-title text-xs text-base-content/60 cursor-pointer hover:text-base-content">
                                Show details
                              </summary>
                              <div className="collapse-content">
                                <pre className="text-xs text-base-content/80 bg-base-200 p-3 rounded-lg overflow-x-auto whitespace-pre-wrap">
                                  {log.details}
                                </pre>
                              </div>
                            </details>
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Summary Statistics */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="card bg-info/10 border border-info/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <Info className="w-5 h-5 text-info" />
            <span className="text-sm font-medium text-info/80 uppercase">Info Messages</span>
          </div>
          <p className="text-2xl font-bold text-info">
            {displayLogs.filter(l => l.level === 'info').length}
          </p>
        </div>
        
        <div className="card bg-warning/10 border border-warning/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <AlertTriangle className="w-5 h-5 text-warning" />
            <span className="text-sm font-medium text-warning/80 uppercase">Warnings</span>
          </div>
          <p className="text-2xl font-bold text-warning">
            {displayLogs.filter(l => l.level === 'warn').length}
          </p>
        </div>
        
        <div className="card bg-error/10 border border-error/20 rounded-xl p-4 hover:scale-105 transition-transform">
          <div className="flex items-center gap-3 mb-2">
            <AlertCircle className="w-5 h-5 text-error" />
            <span className="text-sm font-medium text-error/80 uppercase">Errors</span>
          </div>
          <p className="text-2xl font-bold text-error">
            {displayLogs.filter(l => l.level === 'error').length}
          </p>
        </div>
      </div>
    </div>
  )
}
