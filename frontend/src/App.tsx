import { useEffect, useState } from 'react'
import type { ComponentType } from 'react'
import { useBotStore } from './store/botStore'
import { useSettingsStore } from './store/settingsStore'
import type { SettingsTab } from './store/settingsStore'
import { useWasmStore } from './store/wasmStore'
import { initializeCyberpunkTheme } from './themes/cyberpunkThemes'
import Header from './components/Header'
import Dashboard from './components/Dashboard'
import ConfigurationPanel from './components/ConfigurationPanel'
import HoldingsPanel from './components/HoldingsPanel'
import LogsPanel from './components/LogsPanel'
import NewCoinsPanel from './components/NewCoinsPanel'
import TradingHistory from './components/TradingHistory'
import WalletConnect from './components/WalletConnect'
import ModeSwitcher from './components/ModeSwitcher'
import { ErrorBoundary } from './components/ErrorBoundary'
import { RUNTIME_MODE } from './config'
import { Menu, X, Settings, Home, Wallet, BarChart3, History, TrendingUp, Shield, Bell } from 'lucide-react'
import './App.css'

function App() {
  const { initializeConnection, status, mode, runningState, cleanup } = useBotStore()
  const { activeTab, fetchSettings, setActiveTab } = useSettingsStore()
  const { initializeWasm, initialized: wasmInitialized } = useWasmStore()
  const [isMobileMenuOpen, setIsMobileMenuOpen] = useState(false)
  const [showWalletModal, setShowWalletModal] = useState(false)
  const [showModeModal, setShowModeModal] = useState(false)

  useEffect(() => {
    initializeCyberpunkTheme()
    
    if (RUNTIME_MODE === 'frontend-wasm') {
      initializeWasm().catch(err => console.error('Failed to initialize WASM:', err))
    } else {
      initializeWasm().catch(err => console.error('Failed to initialize WASM:', err))
      initializeConnection()
      fetchSettings()
    }

    return () => cleanup()
  }, [initializeConnection, cleanup, fetchSettings, initializeWasm])

  const navigationItems: Array<{ id: SettingsTab; label: string; icon: ComponentType<Record<string, unknown>> }> = [
    { id: 'dashboard', label: 'Dashboard', icon: Home },
    { id: 'holdings', label: 'Holdings', icon: Wallet },
    { id: 'newcoins', label: 'New Coins', icon: TrendingUp },
    { id: 'trades', label: 'History', icon: History },
    { id: 'configuration', label: 'Settings', icon: Settings },
    { id: 'logs', label: 'Logs', icon: BarChart3 }
  ]

  const renderContent = () => {
    switch (activeTab) {
      case 'dashboard':
        return <Dashboard />
      case 'configuration':
        return <ConfigurationPanel />
      case 'holdings':
        return <HoldingsPanel />
      case 'logs':
        return <LogsPanel />
      case 'newcoins':
        return <NewCoinsPanel />
      case 'trades':
        return <TradingHistory />
      default:
        return <Dashboard />
    }
  }

  return (
    <div className="min-h-screen bg-black relative overflow-hidden">
      {/* Enhanced Animated Background */}
      <div className="fixed inset-0 pointer-events-none">
        <div className="absolute inset-0 opacity-5" style={{
          backgroundImage: `linear-gradient(var(--theme-accent) 1px, transparent 1px), linear-gradient(90deg, var(--theme-accent) 1px, transparent 1px)`,
          backgroundSize: '60px 60px',
        }} />
        <div className="absolute inset-0 bg-gradient-to-br from-transparent via-black/20 to-black/60" />
        <div className="absolute w-full h-px animate-scan-down animate-buzz" style={{
          background: 'linear-gradient(90deg, transparent, var(--theme-accent) 60%, transparent)',
          boxShadow: '0 0 8px var(--glow-color)',
          opacity: 0.18
        }} />
      </div>

      {/* Header */}
      <Header />

      <div className="flex relative z-10">
        {/* Desktop Sidebar */}
        <div className="hidden lg:flex w-48 flex-col border-r border-primary/20 backdrop-blur-sm bg-base-200/30">
          <div className="flex-1 p-4 space-y-3">
            {/* Quick Actions */}
            <div className="space-y-2">
              <button
                onClick={() => setShowWalletModal(true)}
                className="btn btn-sm btn-primary w-full justify-start gap-2 glass-card border border-primary/30 hover:border-primary hover:bg-primary/10 transition-all duration-300"
              >
                <Wallet size={16} />
                <span className="text-xs">Wallet</span>
              </button>
              <button
                onClick={() => setShowModeModal(true)}
                className="btn btn-sm btn-secondary w-full justify-start gap-2 glass-card border border-secondary/30 hover:border-secondary hover:bg-secondary/10 transition-all duration-300"
              >
                <Shield size={16} />
                <span className="text-xs">Mode</span>
              </button>
            </div>

            {/* Navigation */}
            <nav className="space-y-1">
              <div className="text-xs uppercase tracking-wider text-base-content/60 font-semibold px-2 py-1">
                Navigation
              </div>
              {navigationItems.map((item) => {
                const IconComponent = item.icon
                return (
                  <button
                    key={item.id}
                    onClick={() => {
                        setActiveTab(item.id)
                        setIsMobileMenuOpen(false)
                      }}
                    className={`btn btn-xs w-full justify-start gap-2 transition-all duration-300 ${
                      activeTab === item.id
                        ? 'btn-active bg-primary/20 border-primary text-primary'
                        : 'btn-ghost hover:bg-base-100/50 hover:border-base-300'
                    } border border-transparent`}
                  >
                    <IconComponent size={14} />
                    <span className="text-xs">{item.label}</span>
                  </button>
                )
              })}
            </nav>
          </div>

          {/* Status Card */}
          <div className="p-3 border-t border-primary/20">
            <div className="card bg-base-100/50 backdrop-blur border border-primary/30">
              <div className="card-body p-3">
                <div className="flex items-center gap-2 mb-2">
                  <div className={`w-2 h-2 rounded-full ${status === 'connected' ? 'bg-success animate-pulse' : 'bg-error'}`} />
                  <span className="text-xs font-mono uppercase tracking-wider">
                    {status === 'connected' ? 'Online' : 'Offline'}
                  </span>
                </div>
                <div className="text-xs text-base-content/60">
                  <div className="flex justify-between">
                    <span>Bot:</span>
                    <span className={`font-semibold ${runningState === 'running' ? 'text-success' : 'text-warning'}`}>
                      {runningState}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span>Mode:</span>
                    <span className={`font-semibold ${mode === 'real' ? 'text-error' : 'text-info'}`}>
                      {mode}
                    </span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Mobile Menu Overlay */}
        {isMobileMenuOpen && (
          <div className="fixed inset-0 z-50 lg:hidden">
            <div className="fixed inset-0 bg-black/50 backdrop-blur-sm" onClick={() => setIsMobileMenuOpen(false)} />
            <div className="fixed left-0 top-0 h-full w-80 bg-base-200 border-r border-primary/20 p-6">
              <div className="flex justify-between items-center mb-6">
                <h2 className="text-lg font-bold text-primary">Menu</h2>
                <button onClick={() => setIsMobileMenuOpen(false)} className="btn btn-ghost btn-sm">
                  <X size={18} />
                </button>
              </div>
              
              {/* Mobile Navigation */}
              <div className="space-y-3">
                {navigationItems.map((item) => {
                  const IconComponent = item.icon
                  return (
                    <button
                      key={item.id}
                      onClick={() => {
                        setActiveTab(item.id)
                        setIsMobileMenuOpen(false)
                      }}
                      className={`btn btn-block justify-start gap-3 ${
                        activeTab === item.id ? 'btn-active bg-primary/20 text-primary' : 'btn-ghost'
                      }`}
                    >
                      <IconComponent size={18} />
                      {item.label}
                    </button>
                  )
                })}
              </div>

              {/* Mobile Status */}
              <div className="mt-6 p-4 card bg-base-100/50">
                <div className="flex items-center gap-2 mb-2">
                  <div className={`w-2 h-2 rounded-full ${status === 'connected' ? 'bg-success animate-pulse' : 'bg-error'}`} />
                  <span className="text-sm font-mono">System Status</span>
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Main Content */}
        <div className="flex-1 min-h-screen">
          {/* Top Bar - Mobile */}
          <div className="lg:hidden bg-base-200/50 backdrop-blur border-b border-primary/20 p-3">
            <div className="flex items-center justify-between">
              <button
                onClick={() => setIsMobileMenuOpen(true)}
                className="btn btn-ghost btn-xs"
              >
                <Menu size={16} />
              </button>
              
              <div className="flex items-center gap-2">
                {/* Status Indicator */}
                <div className="flex items-center gap-2">
                  <div className={`w-2 h-2 rounded-full ${status === 'connected' ? 'bg-success animate-pulse' : 'bg-error'}`} />
                  <span className="text-xs font-mono">{runningState}</span>
                </div>
                
                {/* Quick Actions */}
                <button onClick={() => setShowWalletModal(true)} className="btn btn-primary btn-xs">
                  <Wallet size={14} />
                </button>
                <button onClick={() => setShowModeModal(true)} className="btn btn-secondary btn-xs">
                  <Shield size={14} />
                </button>
              </div>
            </div>
          </div>

          {/* Content Area */}
          <main className="p-6 max-w-7xl mx-auto">
            <ErrorBoundary>
              {renderContent()}
            </ErrorBoundary>
          </main>
        </div>
      </div>

      {/* Wallet Modal */}
      {showWalletModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
          <div className="card bg-base-100 border border-primary/30 w-full max-w-md max-h-[85vh] overflow-y-auto animate-fade-in-scale">
            <div className="card-body p-4">
              <div className="flex justify-between items-center mb-3">
                <h2 className="card-title text-primary text-lg">Connect Wallet</h2>
                <button onClick={() => setShowWalletModal(false)} className="btn btn-ghost btn-sm">
                  <X size={16} />
                </button>
              </div>
              <WalletConnect />
            </div>
          </div>
        </div>
      )}

      {/* Mode Switcher Modal */}
      {showModeModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
          <div className="card bg-base-100 border border-primary/30 w-full max-w-sm animate-fade-in-scale">
            <div className="card-body p-4">
              <div className="flex justify-between items-center mb-3">
                <h2 className="card-title text-primary text-lg">Modes</h2>
                <button onClick={() => setShowModeModal(false)} className="btn btn-ghost btn-sm">
                  <X size={16} />
                </button>
              </div>
              <ModeSwitcher />
            </div>
          </div>
        </div>
      )}

      {/* Connection Status Toast */}
      {status === 'disconnected' && (
        <div className="fixed top-4 right-4 z-40 max-w-sm">
          <div className="alert alert-error shadow-xl backdrop-blur-sm">
            <Bell className="w-4 h-4" />
            <div>
              <div className="font-bold">Connection Lost</div>
              <div className="text-xs">Attempting to reconnect...</div>
            </div>
          </div>
        </div>
      )}

      {/* WASM Ready Toast */}
      {wasmInitialized && (
        <div className="fixed top-4 left-4 z-40 max-w-sm">
          <div className="alert alert-success shadow-xl backdrop-blur-sm">
            <Shield className="w-4 h-4" />
            <div>
              <div className="font-bold">WASM Ready</div>
              <div className="text-xs">Browser trading mode active</div>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

export default App
