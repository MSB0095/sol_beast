import { useEffect } from 'react'
import { useBotStore } from '../store/botStore'
import { useSettingsStore } from '../store/settingsStore'
import { Activity, Settings, TrendingUp, FileText, Coins, ArrowRightLeft } from 'lucide-react'
import { ThemeSwitcher } from './ThemeSwitcher'

export default function Header() {
  const { status } = useBotStore()
  const { activeTab, setActiveTab } = useSettingsStore()

  useEffect(() => {
    // Keyboard shortcuts
    const handleKeyPress = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey) {
        if (e.key === '1') {
          e.preventDefault()
          setActiveTab('dashboard')
        } else if (e.key === '2') {
          e.preventDefault()
          setActiveTab('configuration')
        } else if (e.key === '3') {
          e.preventDefault()
          setActiveTab('holdings')
        }
      }
    }

    window.addEventListener('keydown', handleKeyPress)
    return () => window.removeEventListener('keydown', handleKeyPress)
  }, [setActiveTab])

  const tabs = [
    { id: 'dashboard', label: 'Dashboard', icon: Activity },
    { id: 'holdings', label: 'Holdings', icon: TrendingUp },
    { id: 'newcoins', label: 'New Coins', icon: Coins },
    { id: 'trades', label: 'Trades', icon: ArrowRightLeft },
    { id: 'logs', label: 'Logs', icon: FileText },
    { id: 'configuration', label: 'Configuration', icon: Settings },
  ]

  return (
    <header className="bg-black border-b-2 border-[var(--theme-accent)] sticky top-0 z-[100] relative overflow-visible" style={{ boxShadow: '0 0 30px var(--glow-color), inset 0 0 30px rgba(0,0,0,0.8)' }}>
      {/* Scan line effect */}
      <div className="absolute inset-0 pointer-events-none opacity-30">
        <div className="absolute top-0 left-0 right-0 h-[1px] bg-gradient-to-r from-transparent via-[var(--theme-accent)] to-transparent animate-pulse"></div>
      </div>
      
      <div className="container mx-auto px-4 relative">
        <div className="flex items-center justify-between py-4">
          <div className="flex items-center gap-4">
            <div className="w-12 h-12 electric-border flex items-center justify-center bg-black relative group cursor-pointer">
              <span className="text-2xl animate-pulse">⚡</span>
              <div className="absolute inset-0 bg-[var(--theme-accent)] opacity-0 group-hover:opacity-20 transition-opacity"></div>
            </div>
            <div>
              <h1 className="text-3xl font-display font-black tracking-wider text-[var(--theme-text-primary)]">
                <span className="glow-text">SOL BEAST</span>
              </h1>
              <p className="text-xs font-mono-tech text-[var(--theme-accent)] uppercase tracking-widest">Trading Bot // v2.0</p>
            </div>
          </div>

          <div className="flex items-center gap-4">
            <div className="flex items-center gap-3 px-5 py-2 bg-black electric-border font-mono-tech text-sm uppercase tracking-wider">
              <div 
                className={`w-3 h-3 ${status === 'connected' ? 'bg-[var(--theme-accent)]' : 'bg-red-500'}`}
                style={{
                  boxShadow: status === 'connected' 
                    ? '0 0 10px var(--glow-color-strong), 0 0 20px var(--glow-color)' 
                    : '0 0 10px rgba(255, 0, 0, 0.8), 0 0 20px rgba(255, 0, 0, 0.4)'
                }}
              ></div>
              <span className={`font-semibold ${status === 'connected' ? 'text-[var(--theme-accent)]' : 'text-red-400'}`}>
                {status === 'connected' ? '[ONLINE]' : '[OFFLINE]'}
              </span>
            </div>
            <ThemeSwitcher />
          </div>
        </div>

        {/* Navigation Tabs */}
        <nav className="flex gap-0 border-t-2 border-[var(--theme-accent)] overflow-x-auto">
          {tabs.map(({ id, label, icon: Icon }, index) => (
            <div key={id} className="flex items-center">
              <button
                onClick={() => setActiveTab(id as any)}
                className={`px-4 sm:px-6 py-3 font-mono-tech text-[10px] sm:text-xs font-semibold uppercase tracking-widest flex items-center gap-2 transition-all duration-200 relative overflow-hidden whitespace-nowrap ${
                  activeTab === id
                    ? 'text-black bg-[var(--theme-accent)]'
                    : 'text-[var(--theme-accent)] bg-black hover:bg-[var(--theme-bg-secondary)]'
                }`}
                style={activeTab === id ? {
                  boxShadow: '0 0 20px var(--glow-color-strong), inset 0 0 20px rgba(0,0,0,0.3)'
                } : {}}
              >
                <Icon size={14} className="sm:w-4 sm:h-4" />
                <span className="hidden sm:inline">{label}</span>
                <span className="sm:hidden">{label.split(' ')[0]}</span>
                {activeTab === id && (
                  <div className="absolute bottom-0 left-0 right-0 h-[2px] bg-black"></div>
                )}
              </button>
              {/* Separator - Electric Divider */}
              {index < tabs.length - 1 && (
                <div 
                  className="h-full w-[1px] relative"
                  style={{
                    background: 'linear-gradient(to bottom, transparent, var(--theme-accent), transparent)',
                    boxShadow: '0 0 5px var(--glow-color)'
                  }}
                >
                  <div 
                    className="absolute top-1/2 left-1/2 w-2 h-2 rounded-full -translate-x-1/2 -translate-y-1/2"
                    style={{
                      backgroundColor: 'var(--theme-accent)',
                      boxShadow: '0 0 8px var(--glow-color-strong)'
                    }}
                  />
                </div>
              )}
            </div>
          ))}
        </nav>
      </div>
    </header>
  )
}
