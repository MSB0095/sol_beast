import { useBotStore } from '../store/botStore'
import { Play, Square, Loader2, Shield, Zap } from 'lucide-react'

export default function BotControl() {
  const { runningState, mode, startBot, stopBot } = useBotStore()

  const isStarting = runningState === 'starting'
  const isStopping = runningState === 'stopping'
  const isRunning = runningState === 'running'
  const isStopped = runningState === 'stopped'
  const isTransitioning = isStarting || isStopping

  const handleStart = async () => {
    if (!isTransitioning && isStopped) {
      await startBot()
    }
  }

  const handleStop = async () => {
    if (!isTransitioning && isRunning) {
      await stopBot()
    }
  }

  return (
    <div className="cyber-card p-6">
      <h3 className="font-display text-lg font-black mb-5 glow-text uppercase tracking-wider">Bot Control</h3>
      
      {/* Status indicator */}
      <div className="mb-6 p-6 glass-card rounded-2xl relative overflow-hidden">
        <div className="flex items-center justify-between">
          <div>
            <p className="font-mono-tech text-xs mb-3 uppercase tracking-widest" style={{ color: 'var(--theme-text-secondary)' }}>System Status</p>
            <p className={`font-display text-2xl font-black uppercase tracking-wider flex items-center gap-3 ${
              isRunning ? 'glow-text' : 
              isTransitioning ? '' : 
              ''
            }`} style={
              isRunning ? { color: 'var(--theme-success)' } : 
              isTransitioning ? { color: 'var(--theme-warning)' } :
              { color: 'var(--theme-text-muted)' }
            }>
              {isRunning && <span className="status-online"></span>}
              {isTransitioning && <span className="status-warning"></span>}
              {isStopped && <span className="status-offline"></span>}
              {isStarting && '[INITIALIZING...]'}
              {isStopping && '[SHUTTING DOWN...]'}
              {isRunning && '[ACTIVE]'}
              {isStopped && '[INACTIVE]'}
            </p>
          </div>
          {isTransitioning && (
            <Loader2 size={32} className="animate-spin" style={{ color: 'var(--theme-warning)', filter: 'drop-shadow(0 0 10px var(--theme-warning))' }} />
          )}
        </div>
      </div>

      {/* Mode Display (read-only) */}
      <div className="mb-6">
        <label className="text-sm mb-2 block uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>Trading Mode</label>
        <div className="grid grid-cols-2 gap-2">
          <div
            className="p-3 rounded-xl border-2 transition-all"
            style={mode === 'dry-run' ? {
              backgroundColor: 'var(--theme-bg-card)',
              borderColor: 'var(--theme-accent)',
              color: 'var(--theme-accent)',
              boxShadow: '0 0 20px var(--glow-color)'
            } : {
              backgroundColor: 'var(--theme-bg-secondary)',
              borderColor: 'var(--theme-text-muted)',
              color: 'var(--theme-text-muted)'
            }}
          >
            <div className="flex items-center justify-center gap-2">
              <Shield size={18} />
              <span className="font-semibold uppercase tracking-wide">Dry Run</span>
            </div>
            <p className="text-xs mt-1 opacity-70 text-center">Simulation only</p>
          </div>
          
          <div
            className="p-3 rounded-xl border-2 transition-all"
            style={mode === 'real' ? {
              backgroundColor: 'var(--theme-bg-card)',
              borderColor: '#ff6b00',
              color: '#ff6b00',
              boxShadow: '0 0 20px rgba(255, 107, 0, 0.6)'
            } : {
              backgroundColor: 'var(--theme-bg-secondary)',
              borderColor: 'var(--theme-text-muted)',
              color: 'var(--theme-text-muted)'
            }}
          >
            <div className="flex items-center justify-center gap-2">
              <Zap size={18} />
              <span className="font-semibold uppercase tracking-wide">Real Trading</span>
            </div>
            <p className="text-xs mt-1 opacity-70 text-center">Live trades</p>
          </div>
        </div>
        <p className="text-xs mt-2 font-mono" style={{ color: 'var(--theme-text-muted)' }}>
          Mode is set at startup with <code className="px-1 rounded" style={{ 
            backgroundColor: 'var(--theme-bg-secondary)',
            color: 'var(--theme-accent)'
          }}>--real</code> flag. Restart the bot to change mode.
        </p>
      </div>

      {/* Warning for real mode */}
      {mode === 'real' && (
        <div className="alert-warning mb-6 p-4 rounded-xl relative overflow-hidden animate-fade-in-up">
          <p className="text-sm flex items-center gap-3 uppercase tracking-widest font-bold">
            <Zap size={20} className="flex-shrink-0 animate-pulse" />
            <span>REAL TRADING MODE ACTIVE • LIVE SOL TRANSACTIONS</span>
          </p>
        </div>
      )}

      {/* Control Buttons */}
      <div className="flex gap-3">
        <button
          onClick={handleStart}
          disabled={!isStopped || isTransitioning}
          className="flex-1 py-3 px-4 rounded-xl transition-all flex items-center justify-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
          style={!isStopped || isTransitioning ? {} : {
            background: 'linear-gradient(135deg, #10b981 0%, #059669 100%)',
            color: '#000000',
            border: '2px solid #10b981',
            boxShadow: '0 0 20px rgba(16, 185, 129, 0.5)'
          }}
        >
          {isStarting ? (
            <>
              <Loader2 size={20} className="animate-spin" />
              STARTING...
            </>
          ) : (
            <>
              <Play size={20} />
              START BOT
            </>
          )}
        </button>

        <button
          onClick={handleStop}
          disabled={!isRunning || isTransitioning}
          className="flex-1 py-3 px-4 rounded-xl transition-all flex items-center justify-center gap-2 disabled:opacity-50 disabled:cursor-not-allowed"
          style={!isRunning || isTransitioning ? {} : {
            background: 'linear-gradient(135deg, #ef4444 0%, #dc2626 100%)',
            color: '#ffffff',
            border: '2px solid #ef4444',
            boxShadow: '0 0 20px rgba(239, 68, 68, 0.5)'
          }}
        >
          {isStopping ? (
            <>
              <Loader2 size={20} className="animate-spin" />
              STOPPING...
            </>
          ) : (
            <>
              <Square size={20} />
              STOP BOT
            </>
          )}
        </button>
      </div>

      {/* Info text */}
      <div className="mt-4 text-xs font-mono" style={{ color: 'var(--theme-text-muted)' }}>
        <p>• START THE BOT TO BEGIN MONITORING AND TRADING</p>
        <p>• STOP THE BOT TO PAUSE ALL OPERATIONS</p>
        <p>• SWITCH MODE ONLY WHEN BOT IS STOPPED</p>
      </div>
    </div>
  )
}
