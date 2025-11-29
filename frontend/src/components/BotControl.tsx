import { useBotStore } from '../store/botStore'
import { Play, Square, Loader2, Shield, Zap, Activity, AlertTriangle } from 'lucide-react'

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
    <div className="card bg-base-200/50 border border-base-300 rounded-xl">
      <div className="card-body">
        {/* Header */}
        <div className="flex items-center gap-3 mb-6">
          <div className="p-3 bg-primary/10 rounded-lg">
            <Activity className="w-6 h-6 text-primary" />
          </div>
          <div>
            <h3 className="card-title text-xl font-bold uppercase tracking-wider">
              Bot Control
            </h3>
            <p className="text-base-content/60">Manage your trading bot operations</p>
          </div>
        </div>

        {/* Status Indicator */}
        <div className="card bg-base-100 border border-base-300 rounded-lg p-4 mb-6">
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium uppercase tracking-wide text-base-content/60 mb-2">
                System Status
              </p>
              <div className="flex items-center gap-3">
                <div className={`w-3 h-3 rounded-full ${
                  isRunning ? 'bg-success animate-pulse' :
                  isTransitioning ? 'bg-warning animate-pulse' :
                  'bg-base-300'
                }`}></div>
                <p className={`text-xl font-bold uppercase tracking-wide ${
                  isRunning ? 'text-success' :
                  isTransitioning ? 'text-warning' :
                  'text-base-content/60'
                }`}>
                  {isStarting && 'INITIALIZING...'}
                  {isStopping && 'SHUTTING DOWN...'}
                  {isRunning && 'ACTIVE'}
                  {isStopped && 'INACTIVE'}
                </p>
              </div>
            </div>
            {isTransitioning && (
              <Loader2 className="w-8 h-8 animate-spin text-warning" />
            )}
          </div>
        </div>

        {/* Mode Display */}
        <div className="mb-6">
          <label className="label-text mb-3 block uppercase tracking-wide">
            Trading Mode
          </label>
          <div className="join join-vertical w-full">
            <div
              className={`join-item p-4 border-2 rounded-lg transition-all cursor-pointer ${
                mode === 'dry-run'
                  ? 'border-primary bg-primary/10 text-primary'
                  : 'border-base-300 bg-base-100/50 text-base-content/60'
              }`}
            >
              <div className="flex items-center gap-3">
                <Shield className="w-5 h-5" />
                <div>
                  <div className="font-semibold uppercase tracking-wide">Dry Run</div>
                  <div className="text-xs opacity-70">Simulation only</div>
                </div>
              </div>
            </div>
            
            <div
              className={`join-item p-4 border-2 rounded-lg transition-all cursor-pointer ${
                mode === 'real'
                  ? 'border-warning bg-warning/10 text-warning'
                  : 'border-base-300 bg-base-100/50 text-base-content/60'
              }`}
            >
              <div className="flex items-center gap-3">
                <Zap className="w-5 h-5" />
                <div>
                  <div className="font-semibold uppercase tracking-wide">Real Trading</div>
                  <div className="text-xs opacity-70">Live trades</div>
                </div>
              </div>
            </div>
          </div>
          <p className="text-xs mt-3 text-base-content/60">
            Mode is set at startup with <code className="px-2 py-1 bg-base-300 rounded text-primary">--real</code> flag.
            Restart the bot to change mode.
          </p>
        </div>

        {/* Warning for real mode */}
        {mode === 'real' && (
          <div role="alert" className="alert alert-warning mb-6">
            <AlertTriangle className="w-5 h-5" />
            <div>
              <h3 className="font-bold uppercase tracking-wider">REAL TRADING MODE ACTIVE</h3>
              <div className="text-xs">LIVE SOL TRANSACTIONS</div>
            </div>
          </div>
        )}

        {/* Control Buttons */}
        <div className="grid grid-cols-2 gap-3">
          <button
            onClick={handleStart}
            disabled={!isStopped || isTransitioning}
            className={`btn btn-success gap-2 ${
              !isStopped || isTransitioning ? 'btn-disabled' : ''
            }`}
          >
            {isStarting ? (
              <>
                <Loader2 className="w-5 h-5 animate-spin" />
                STARTING...
              </>
            ) : (
              <>
                <Play className="w-5 h-5" />
                START BOT
              </>
            )}
          </button>

          <button
            onClick={handleStop}
            disabled={!isRunning || isTransitioning}
            className={`btn btn-error gap-2 ${
              !isRunning || isTransitioning ? 'btn-disabled' : ''
            }`}
          >
            {isStopping ? (
              <>
                <Loader2 className="w-5 h-5 animate-spin" />
                STOPPING...
              </>
            ) : (
              <>
                <Square className="w-5 h-5" />
                STOP BOT
              </>
            )}
          </button>
        </div>

        {/* Info text */}
        <div className="mt-6 p-4 bg-base-100/50 rounded-lg">
          <p className="text-xs text-base-content/60 space-y-1">
            <span>• START THE BOT TO BEGIN MONITORING AND TRADING</span>
            <span>• STOP THE BOT TO PAUSE ALL OPERATIONS</span>
            <span>• SWITCH MODE ONLY WHEN BOT IS STOPPED</span>
          </p>
        </div>
      </div>
    </div>
  )
}
