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
import './App.css'

function App() {
  const { initializeConnection, status, mode, runningState } = useBotStore()
  const { activeTab, fetchSettings } = useSettingsStore()

  useEffect(() => {
    initializeConnection()
    fetchSettings()
  }, [initializeConnection, fetchSettings])

  return (
    <div className="min-h-screen bg-gradient-to-br from-sol-darker via-sol-dark to-sol-darker">
      <Header />
      
      <main className="container mx-auto px-4 py-8">
        {status === 'disconnected' && (
          <div className="mb-4 p-4 bg-red-900/20 border border-red-500 rounded-lg text-red-200">
            <p className="font-semibold">Connection Status: Disconnected</p>
            <p className="text-sm">Trying to connect to backend at http://localhost:8080...</p>
          </div>
        )}

        {status === 'connected' && (
          <div className="mb-4 p-4 bg-green-900/20 border border-green-500 rounded-lg text-green-200 flex items-center justify-between">
            <div>
              <p className="font-semibold">✓ Connected to Trading Bot</p>
              <p className="text-sm mt-1">
                Mode: <span className="font-mono">{mode}</span> • 
                Status: <span className="font-mono">{runningState}</span>
              </p>
            </div>
          </div>
        )}

        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          {/* Main Content Area */}
          <div className="lg:col-span-2">
            {activeTab === 'dashboard' && <Dashboard />}
            {activeTab === 'configuration' && <ConfigurationPanel />}
            {activeTab === 'holdings' && <HoldingsPanel />}
            {activeTab === 'logs' && <LogsPanel />}
            {activeTab === 'newcoins' && <NewCoinsPanel />}
            {activeTab === 'trades' && <TradingHistory />}
          </div>

          {/* Sidebar */}
          <div className="lg:col-span-1 space-y-6">
            {/* Bot Control */}
            <BotControl />
            
            {/* Quick Stats */}
            <div className="bg-sol-dark rounded-lg border border-gray-700 p-6 sticky top-24">
              <h3 className="text-lg font-semibold mb-4 text-sol-purple">Quick Stats</h3>
              <div className="space-y-3 text-sm">
                <div className="flex justify-between">
                  <span className="text-gray-400">Connection:</span>
                  <span className={status === 'connected' ? 'text-green-400' : 'text-red-400'}>
                    {status === 'connected' ? 'Online' : 'Offline'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Bot State:</span>
                  <span className={runningState === 'running' ? 'text-green-400' : 'text-gray-400'}>
                    {runningState}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-gray-400">Trading Mode:</span>
                  <span className={mode === 'real' ? 'text-orange-400' : 'text-blue-400'}>
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
