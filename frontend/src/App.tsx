import { useEffect } from 'react'
import { useBotStore } from './store/botStore'
import { useSettingsStore } from './store/settingsStore'
import Header from './components/Header'
import Dashboard from './components/Dashboard'
import ConfigurationPanel from './components/ConfigurationPanel'
import HoldingsPanel from './components/HoldingsPanel'
import LogsPanel from './components/LogsPanel'
import BotControl from './components/BotControl'
import NewCoinsPanel from './components/NewCoinsPanel'
import TradingHistory from './components/TradingHistory'
import ProfilePanel from './components/ProfilePanel'
import { ErrorBoundary } from './components/ErrorBoundary'
import './App.css'

function App() {
  const { initializeConnection, status, mode, runningState, cleanup } = useBotStore()
  const { activeTab, fetchSettings } = useSettingsStore()

  useEffect(() => {
    initializeConnection()
    fetchSettings()

    // Cleanup on unmount
    return () => {
      cleanup()
    }
  }, [initializeConnection, cleanup, fetchSettings])

  return (
    <div className="min-h-screen bg-black transition-colors duration-500 relative overflow-hidden">
      {/* Animated electric grid background */}
      <div className="fixed inset-0 opacity-10 pointer-events-none animate-pulse" style={{
        backgroundImage: `linear-gradient(var(--theme-accent) 1px, transparent 1px), linear-gradient(90deg, var(--theme-accent) 1px, transparent 1px)`,
        backgroundSize: '50px 50px',
        animation: 'pulse 4s cubic-bezier(0.4, 0, 0.6, 1) infinite'
      }}></div>
      
      {/* Radial gradient overlay for depth */}
      <div className="fixed inset-0 pointer-events-none" style={{
        background: 'radial-gradient(circle at 50% 50%, transparent 0%, rgba(0, 0, 0, 0.8) 100%)'
      }}></div>
      
      {/* Animated scanline */}
      <div className="fixed inset-0 pointer-events-none overflow-hidden">
        <div 
          className="absolute w-full h-[2px] animate-scan-down"
          style={{
            background: 'linear-gradient(90deg, transparent, var(--theme-accent), transparent)',
            boxShadow: '0 0 20px var(--glow-color-strong)',
            opacity: 0.3
          }}
        />
      </div>
      
      <Header />
      
      <main className="container mx-auto px-4 py-8 relative z-10">
        {status === 'disconnected' && (
          <div className="alert-error mb-6 p-6 rounded-2xl relative overflow-hidden animate-fade-in-up">
            <p className="font-mono-tech font-black flex items-center gap-3 uppercase tracking-widest text-base mb-3">
              <span className="status-offline"></span>
              [CONNECTION LOST]
            </p>
            <p className="font-mono-tech text-sm opacity-90">ATTEMPTING RECONNECT TO BACKEND @ http://localhost:8080...</p>
            <div className="mt-4 h-1 bg-black/30 rounded-full overflow-hidden">
              <div className="h-full bg-[var(--theme-error)] rounded-full animate-pulse" style={{ width: '60%' }}></div>
            </div>
          </div>
        )}

        {status === 'connected' && (
          <div className="mb-6 p-5 bg-black electric-border flex items-center justify-between relative overflow-hidden cyber-card">
            <div>
              <p className="font-mono-tech font-bold flex items-center gap-3 uppercase tracking-wider text-sm">
                <span className="w-3 h-3 bg-[var(--theme-accent)] animate-pulse" style={{ boxShadow: '0 0 15px var(--glow-color-strong)' }}></span>
                <span className="glow-text">[CONNECTED] TRADING BOT ACTIVE</span>
              </p>
              <p className="font-mono-tech text-xs mt-3 text-[var(--theme-text-secondary)]">
                MODE: <span className="text-[var(--theme-accent)] font-bold px-2 py-1 bg-[var(--theme-bg-card)]">{mode.toUpperCase()}</span> • 
                STATUS: <span className="text-[var(--theme-accent)] font-bold px-2 py-1 bg-[var(--theme-bg-card)] ml-2">{runningState.toUpperCase()}</span>
              </p>
            </div>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Main Content Area */}
          <div className="lg:col-span-2">
            {activeTab === 'dashboard' && (
              <ErrorBoundary>
                <Dashboard />
              </ErrorBoundary>
            )}
            {activeTab === 'configuration' && (
              <ErrorBoundary>
                <ConfigurationPanel />
              </ErrorBoundary>
            )}
            {activeTab === 'holdings' && (
              <ErrorBoundary>
                <HoldingsPanel />
              </ErrorBoundary>
            )}
            {activeTab === 'logs' && (
              <ErrorBoundary>
                <LogsPanel />
              </ErrorBoundary>
            )}
            {activeTab === 'newcoins' && (
              <ErrorBoundary>
                <NewCoinsPanel />
              </ErrorBoundary>
            )}
            {activeTab === 'trades' && (
              <ErrorBoundary>
                <TradingHistory />
              </ErrorBoundary>
            )}
            {activeTab === 'profile' && (
              <ErrorBoundary>
                <ProfilePanel />
              </ErrorBoundary>
            )}
          </div>

          {/* Sidebar */}
          <div className="lg:col-span-1 space-y-6">
            {/* Bot Control */}
            <BotControl />
            
            {/* Quick Stats */}
            <div className="cyber-card p-6">
              <h3 className="font-display text-xl font-black mb-5 glow-text uppercase tracking-wider">Quick Stats</h3>
              <div className="space-y-3 font-mono-tech text-xs">
                <div className="flex justify-between items-center gap-3 p-3 bg-black electric-border">
                  <span className="text-[var(--theme-text-secondary)] uppercase tracking-wider whitespace-nowrap">Connection:</span>
                  <span className={`font-bold uppercase tracking-widest whitespace-nowrap ${status === 'connected' ? 'text-[var(--theme-accent)]' : 'text-red-400'}`}>
                    {status === 'connected' ? '[ONLINE]' : '[OFFLINE]'}
                  </span>
                </div>
                <div className="flex justify-between items-center gap-3 p-3 bg-black electric-border">
                  <span className="text-[var(--theme-text-secondary)] uppercase tracking-wider whitespace-nowrap">Bot State:</span>
                  <span className={`font-bold uppercase tracking-widest whitespace-nowrap ${runningState === 'running' ? 'text-[var(--theme-accent)]' : 'text-[var(--theme-text-secondary)]'}`}>
                    [{runningState.toUpperCase()}]
                  </span>
                </div>
                <div className="flex justify-between items-center gap-3 p-3 bg-black electric-border">
                  <span className="text-[var(--theme-text-secondary)] uppercase tracking-wider whitespace-nowrap">Mode:</span>
                  <span className={`font-bold uppercase tracking-widest whitespace-nowrap ${mode === 'real' ? 'text-orange-400' : 'text-blue-400'}`}>
                    {mode === 'real' ? '[REAL]' : '[DRY-RUN]'}
                  </span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  )
}

export default App
