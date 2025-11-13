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
    <div className="card-enhanced rounded-xl p-6">
      <h3 className="text-lg font-semibold mb-4 gradient-text">Bot Control</h3>
      
      {/* Status indicator */}
      <div className="mb-6 p-4 rounded-xl bg-gradient-to-r from-sol-darker to-gray-900 border border-gray-700/50">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm text-gray-400 mb-1">Status</p>
            <p className={`text-lg font-bold ${
              isRunning ? 'text-green-400' : 
              isTransitioning ? 'text-yellow-400' : 
              'text-gray-400'
            }`}>
              {isStarting && 'Starting...'}
              {isStopping && 'Stopping...'}
              {isRunning && 'Running'}
              {isStopped && 'Stopped'}
            </p>
          </div>
          {isTransitioning && (
            <Loader2 size={24} className="text-yellow-400 animate-spin" />
          )}
          {isRunning && (
            <div className="w-3 h-3 bg-green-500 rounded-full animate-pulse shadow-glow"></div>
          )}
        </div>
      </div>

      {/* Mode Display (read-only) */}
      <div className="mb-6">
        <label className="text-sm text-gray-400 mb-2 block">Trading Mode</label>
        <div className="grid grid-cols-2 gap-2">
          <div
            className={`p-3 rounded-xl border transition-all ${
              mode === 'dry-run'
                ? 'bg-blue-500/20 border-blue-500/50 text-blue-300 shadow-card'
                : 'bg-gray-800/50 border-gray-700 text-gray-500'
            }`}
          >
            <div className="flex items-center justify-center gap-2">
              <Shield size={18} />
              <span className="font-semibold">Dry Run</span>
            </div>
            <p className="text-xs mt-1 opacity-70">Simulation only</p>
          </div>
          
          <div
            className={`p-3 rounded-xl border transition-all ${
              mode === 'real'
                ? 'bg-orange-500/20 border-orange-500/50 text-orange-300 shadow-card'
                : 'bg-gray-800/50 border-gray-700 text-gray-500'
            }`}
          >
            <div className="flex items-center justify-center gap-2">
              <Zap size={18} />
              <span className="font-semibold">Real Trading</span>
            </div>
            <p className="text-xs mt-1 opacity-70">Live trades</p>
          </div>
        </div>
        <p className="text-xs text-gray-500 mt-2">
          Mode is set at startup with <code className="bg-gray-800 px-1 rounded">--real</code> flag. Restart the bot to change mode.
        </p>
      </div>

      {/* Warning for real mode */}
      {mode === 'real' && (
        <div className="mb-6 p-3 bg-orange-500/10 border border-orange-500/30 rounded-xl backdrop-blur-sm">
          <p className="text-orange-300 text-sm flex items-start gap-2">
            <Zap size={16} className="flex-shrink-0 mt-0.5" />
            <span>Real trading mode is active. Trades will use actual SOL.</span>
          </p>
        </div>
      )}

      {/* Control Buttons */}
      <div className="flex gap-3">
        <button
          onClick={handleStart}
          disabled={!isStopped || isTransitioning}
          className="flex-1 py-3 px-4 bg-gradient-to-r from-green-500 to-green-600 hover:from-green-600 hover:to-green-700 disabled:bg-gray-700 disabled:cursor-not-allowed text-white font-semibold rounded-xl transition-all flex items-center justify-center gap-2 disabled:opacity-50 shadow-card hover:shadow-card-hover hover:scale-105"
        >
          {isStarting ? (
            <>
              <Loader2 size={20} className="animate-spin" />
              Starting...
            </>
          ) : (
            <>
              <Play size={20} />
              Start Bot
            </>
          )}
        </button>

        <button
          onClick={handleStop}
          disabled={!isRunning || isTransitioning}
          className="flex-1 py-3 px-4 bg-gradient-to-r from-red-500 to-red-600 hover:from-red-600 hover:to-red-700 disabled:bg-gray-700 disabled:cursor-not-allowed text-white font-semibold rounded-xl transition-all flex items-center justify-center gap-2 disabled:opacity-50 shadow-card hover:shadow-card-hover hover:scale-105"
        >
          {isStopping ? (
            <>
              <Loader2 size={20} className="animate-spin" />
              Stopping...
            </>
          ) : (
            <>
              <Square size={20} />
              Stop Bot
            </>
          )}
        </button>
      </div>

      {/* Info text */}
      <div className="mt-4 text-xs text-gray-500">
        <p>• Start the bot to begin monitoring and trading</p>
        <p>• Stop the bot to pause all operations</p>
        <p>• Switch mode only when bot is stopped</p>
      </div>
    </div>
  )
}
