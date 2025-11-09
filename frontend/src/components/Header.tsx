import { useEffect } from 'react'
import { useBotStore } from '../store/botStore'
import { useSettingsStore } from '../store/settingsStore'
import { Activity, Settings, TrendingUp, FileText, Coins, ArrowRightLeft } from 'lucide-react'

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
    <header className="bg-sol-dark border-b border-gray-700 sticky top-0 z-50">
      <div className="container mx-auto px-4">
        <div className="flex items-center justify-between py-4">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-lg bg-gradient-to-br from-sol-purple to-purple-600 flex items-center justify-center">
              <span className="text-black font-bold text-lg">🚀</span>
            </div>
            <div>
              <h1 className="text-2xl font-bold text-white">Sol Beast</h1>
              <p className="text-xs text-gray-400">Solana Trading Bot Dashboard</p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <div className={`w-3 h-3 rounded-full ${status === 'connected' ? 'bg-green-500' : 'bg-red-500'} animate-pulse`}></div>
            <span className="text-sm text-gray-300">{status === 'connected' ? 'Connected' : 'Disconnected'}</span>
          </div>
        </div>

        {/* Navigation Tabs */}
        <nav className="flex gap-1 border-t border-gray-700">
          {tabs.map(({ id, label, icon: Icon }) => (
            <button
              key={id}
              onClick={() => setActiveTab(id as any)}
              className={`px-4 py-3 text-sm font-medium flex items-center gap-2 transition-all ${
                activeTab === id
                  ? 'text-sol-purple border-b-2 border-sol-purple'
                  : 'text-gray-400 hover:text-gray-300'
              }`}
            >
              <Icon size={18} />
              {label}
            </button>
          ))}
        </nav>
      </div>
    </header>
  )
}
