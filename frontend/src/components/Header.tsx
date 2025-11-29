import { useBotStore } from '../store/botStore'
import { useSettingsStore } from '../store/settingsStore'
import { ThemeSwitcher } from './ThemeSwitcher'
import BeastLogo from './BeastLogo'
import { useWasmStore } from '../store/wasmStore'

export default function Header() {
  const { status, runningState, initializeConnection, startBot, stopBot } = useBotStore()
  const settingsStore = useSettingsStore()
  const wasmStore = useWasmStore()

  const getCurrentTabLabel = () => {
    const tabLabels: Record<string, string> = {
      dashboard: 'Command Center',
      holdings: 'Portfolio',
      newcoins: 'Opportunities',
      trades: 'History',
      configuration: 'Settings',
      logs: 'Monitoring'
    }
    return tabLabels[settingsStore.activeTab] || 'Dashboard'
  }

  return (
    <header className="bg-black/90 backdrop-blur-sm border-b border-primary/30 sticky top-0 z-50 relative overflow-hidden">
      {/* Enhanced background effects */}
      <div className="absolute inset-0 pointer-events-none">
        <div className="absolute top-0 left-0 right-0 h-px bg-gradient-to-r from-transparent via-primary to-transparent opacity-60" />
        <div className="absolute inset-0 bg-gradient-to-r from-transparent via-primary/5 to-transparent" />
      </div>
      
      <div className="container mx-auto px-6 py-6 relative z-10">
        <div className="flex items-center justify-between">
          {/* Left: Logo & Branding */}
          <div className="flex items-center gap-8">
            <div className="relative group">
              <div className="absolute inset-0 bg-primary/20 rounded-2xl blur-2xl group-hover:blur-3xl transition-all duration-300 opacity-0 group-hover:opacity-100" />
              <div className="relative p-2 rounded-xl border border-primary/20 bg-primary/5 hover:bg-primary/10 transition-all duration-200 shadow-sm">
                <BeastLogo size={48} animated={false} />
              </div>
            </div>
            
            <div>
              <h1 className="text-xl lg:text-2xl font-bold text-primary uppercase tracking-wide mb-1">
                SOL BEAST
              </h1>
              {/* Small status dot for brand */}
              <div className="flex items-center gap-2">
                <div className="w-3 h-3 bg-primary rounded-full" />
                <span className="text-xs text-base-content/60 uppercase tracking-wider">{getCurrentTabLabel()}</span>
              </div>
              <p className="text-sm font-mono text-primary/70 uppercase tracking-widest flex items-center gap-3">
                <span className="w-3 h-3 bg-success rounded-full animate-buzz animate-fuzz shadow-sm shadow-success/40" />
                {getCurrentTabLabel()}
                <span className="mx-3 text-primary/40">•</span>
                <span className="text-xs">v2.0</span>
              </p>
            </div>
          </div>

          {/* Center: Quick Stats */}
            <div className="hidden xl:flex items-center gap-6">
            <div className="flex items-center gap-4 px-6 py-3 rounded-xl bg-base-200/60 border border-primary/30 backdrop-blur-sm shadow-sm">
              <div className={`w-3 h-3 rounded-full ${status === 'connected' ? 'bg-success animate-buzz' : 'bg-error'}`} />
              <span className="text-sm font-mono uppercase tracking-wider text-base-content/90 font-semibold">
                {status === 'connected' ? 'Connected' : 'Disconnected'}
              </span>
            </div>
            
            <div className="flex items-center gap-4 px-6 py-3 rounded-xl bg-base-200/60 border border-primary/30 backdrop-blur-sm shadow-sm">
              <div className={`w-3 h-3 rounded-full ${runningState === 'running' ? 'bg-warning animate-buzz' : 'bg-base-content/40'}`} />
              <span className="text-sm font-mono uppercase tracking-wider text-base-content/90 font-semibold">
                {runningState}
              </span>
            </div>
          </div>

          {/* Right: Controls */}
          <div className="flex items-center gap-4">
            {/* Current Time */}
            <div className="hidden lg:block text-sm font-mono text-base-content/70 px-3 py-2 rounded-lg bg-base-200/40 border border-primary/10">
              {new Date().toLocaleTimeString('en-US', {
                hour12: false,
                hour: '2-digit',
                minute: '2-digit',
                second: '2-digit'
              })}
            </div>

            {/* Theme Switcher */}
            <div className="relative">
              <ThemeSwitcher />
            </div>
            {/* Engine selector + connect/start controls */}
            <div className="flex items-center gap-3">
              <div className="hidden md:flex items-center gap-2 border border-primary/20 rounded-lg px-2 py-1 bg-base-200/20">
                <button
                  onClick={() => settingsStore.setEngine('backend')}
                  className={`px-3 py-1 rounded ${settingsStore.engine === 'backend' ? 'bg-primary text-black' : 'bg-transparent text-primary/70 hover:bg-primary/5'}`}
                >
                  Backend
                </button>
                <button
                  onClick={() => settingsStore.setEngine('wasm')}
                  className={`px-3 py-1 rounded ${settingsStore.engine === 'wasm' ? 'bg-primary text-black' : 'bg-transparent text-primary/70 hover:bg-primary/5'}`}
                >
                  WASM
                </button>
              </div>

              <div>
                {settingsStore.engine === 'wasm' ? (
                  wasmStore.initialized ? (
                    <div className="flex items-center gap-2">
                      <button
                        onClick={() => wasmStore.startBot()}
                        className="px-3 py-1 rounded bg-primary text-black text-xs font-sans"
                      >
                        Start WASM
                      </button>
                      <button
                        onClick={() => wasmStore.stopBot()}
                        className="px-3 py-1 rounded bg-error text-black text-xs font-sans"
                      >
                        Stop WASM
                      </button>
                    </div>
                  ) : (
                    <button
                      onClick={() => wasmStore.initializeWasm()}
                      className="px-3 py-1 rounded bg-primary text-black text-xs font-sans"
                    >
                      Initialize WASM
                    </button>
                  )
                ) : (
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => initializeConnection()}
                      className="px-3 py-1 rounded bg-primary text-black text-xs font-sans"
                    >
                      Connect
                    </button>
                    {runningState !== 'running' ? (
                      <button
                        onClick={() => startBot()}
                        className="px-3 py-1 rounded bg-primary text-black text-xs font-sans"
                      >
                        Start
                      </button>
                    ) : (
                      <button
                        onClick={() => stopBot()}
                        className="px-3 py-1 rounded bg-error text-black text-xs font-sans"
                      >
                        Stop
                      </button>
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>

      {/* Minimal scan line animation (subtle) */}
      <div className="absolute inset-0 pointer-events-none overflow-hidden">
        <div
          className="absolute top-0 left-0 w-full h-px bg-gradient-to-r from-transparent via-primary/30 to-transparent opacity-20"
          style={{
            animation: 'scan-down 18s linear infinite',
            boxShadow: '0 0 6px var(--glow-color)'
          }}
        />
      </div>

      {/* Keyboard shortcut hints */}
      <div className="absolute bottom-2 right-6 text-xs text-base-content/40 font-mono">
        <span className="hidden xl:inline">Ctrl+1-6 • </span>
        <span className="hidden xl:inline">Ctrl+Cmd+1-6</span>
      </div>
    </header>
  )
}
