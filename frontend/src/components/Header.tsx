import { useEffect } from 'react'
import { useBotStore } from '../store/botStore'
import { useSettingsStore } from '../store/settingsStore'
import { Activity, Settings, TrendingUp, FileText, Coins, ArrowRightLeft, User } from 'lucide-react'
import { ThemeSwitcher } from './ThemeSwitcher'
import BeastLogo from './BeastLogo'
import WalletButton from './WalletButton'

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
    { id: 'profile', label: 'Profile', icon: User },
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
            <div className="w-14 h-14 electric-border flex items-center justify-center bg-black relative group cursor-pointer overflow-hidden">
              <BeastLogo size={48} animated={true} />
              <div className="absolute inset-0 bg-[var(--theme-accent)] opacity-0 group-hover:opacity-20 transition-opacity"></div>
              {/* Animated border scan */}
              <div className="absolute inset-0 pointer-events-none">
                <div className="absolute top-0 left-0 w-full h-[2px] bg-gradient-to-r from-transparent via-[var(--theme-accent)] to-transparent animate-scan-horizontal"></div>
                <div className="absolute left-0 top-0 w-[2px] h-full bg-gradient-to-b from-transparent via-[var(--theme-accent)] to-transparent animate-scan-vertical"></div>
              </div>
            </div>
            <div>
              <h1 className="text-3xl md:text-4xl font-display font-black tracking-wider text-[var(--theme-text-primary)] relative">
                <span className="glow-text relative inline-block">
                  SOL BEAST
                  {/* Glitch effect overlay */}
                  <span className="absolute top-0 left-0 w-full h-full text-[var(--theme-error)] opacity-0 animate-glitch-1" aria-hidden="true">SOL BEAST</span>
                  <span className="absolute top-0 left-0 w-full h-full text-[var(--theme-info)] opacity-0 animate-glitch-2" aria-hidden="true">SOL BEAST</span>
                </span>
              </h1>
              <p className="text-xs font-mono-tech text-[var(--theme-accent)] uppercase tracking-widest flex items-center gap-2">
                <span className="inline-block w-2 h-2 bg-[var(--theme-accent)] rounded-full animate-pulse"></span>
                Memecoins Sniper // v1.0 <span className="text-[var(--theme-info)]"></span>
              </p>
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
            <WalletButton />
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
