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
    <div className="min-h-screen bg-gradient-to-br from-sol-darker via-sol-dark to-sol-darker animate-gradient">
      <Header />
      
      <main className="container mx-auto px-4 py-8">
        {status === 'disconnected' && (
          <div className="mb-4 p-4 bg-red-900/20 border border-red-500/50 rounded-xl text-red-200 backdrop-blur-sm shadow-card">
            <p className="font-semibold flex items-center gap-2">
              <span className="w-2 h-2 bg-red-500 rounded-full animate-pulse"></span>
              Connection Status: Disconnected
            </p>
            <p className="text-sm mt-2 text-red-300/80">Trying to connect to backend at http://localhost:8080...</p>
          </div>
        )}

        {status === 'connected' && (
          <div className="mb-4 p-4 bg-green-900/20 border border-green-500/50 rounded-xl text-green-200 flex items-center justify-between backdrop-blur-sm shadow-card glow-on-hover">
            <div>
              <p className="font-semibold flex items-center gap-2">
                <span className="w-2 h-2 bg-green-500 rounded-full animate-pulse shadow-glow"></span>
                ✓ Connected to Trading Bot
              </p>
              <p className="text-sm mt-2 text-green-300/80">
                Mode: <span className="font-mono bg-green-950/30 px-2 py-0.5 rounded">{mode}</span> • 
                Status: <span className="font-mono bg-green-950/30 px-2 py-0.5 rounded">{runningState}</span>
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
          </div>

          {/* Sidebar */}
          <div className="lg:col-span-1 space-y-6">
            {/* Bot Control */}
            <BotControl />
            
            {/* Quick Stats */}
            <div className="card-enhanced rounded-xl p-6 sticky top-24">
              <h3 className="text-lg font-semibold mb-4 gradient-text">Quick Stats</h3>
              <div className="space-y-4 text-sm">
                <div className="flex justify-between items-center p-2 rounded-lg bg-sol-darker/50">
                  <span className="text-gray-400">Connection:</span>
                  <span className={`font-semibold ${status === 'connected' ? 'text-green-400' : 'text-red-400'}`}>
                    {status === 'connected' ? '● Online' : '○ Offline'}
                  </span>
                </div>
                <div className="flex justify-between items-center p-2 rounded-lg bg-sol-darker/50">
                  <span className="text-gray-400">Bot State:</span>
                  <span className={`font-semibold ${runningState === 'running' ? 'text-green-400' : 'text-gray-400'}`}>
                    {runningState}
                  </span>
                </div>
                <div className="flex justify-between items-center p-2 rounded-lg bg-sol-darker/50">
                  <span className="text-gray-400">Trading Mode:</span>
                  <span className={`font-semibold ${mode === 'real' ? 'text-orange-400' : 'text-blue-400'}`}>
                    {mode === 'real' ? '⚡ Real' : '🛡️ Dry-Run'}
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
