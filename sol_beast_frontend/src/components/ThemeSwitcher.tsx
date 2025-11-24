import { useState, useEffect } from 'react'

const COLOR_THEMES = [
  { 
    id: 'sol-green', 
    name: 'MATRIX', 
    primaryColor: '#00ff41',
    icon: '◉'
  },
  { 
    id: 'emerald', 
    name: 'NEON', 
    primaryColor: '#10ffb0',
    icon: '◈'
  },
  { 
    id: 'blue', 
    name: 'CYBER', 
    primaryColor: '#00d9ff',
    icon: '◆'
  },
  { 
    id: 'purple', 
    name: 'PLASMA', 
    primaryColor: '#d946ef',
    icon: '◇'
  },
  { 
    id: 'rose', 
    name: 'LASER', 
    primaryColor: '#ff0062',
    icon: '◊'
  },
  { 
    id: 'amber', 
    name: 'GOLD', 
    primaryColor: '#ffb000',
    icon: '◐'
  },
  { 
    id: 'cyan', 
    name: 'TRON', 
    primaryColor: '#00ffff',
    icon: '◑'
  },
]

export function ThemeSwitcher() {
  const [currentTheme, setCurrentTheme] = useState('sol-green')
  const [isOpen, setIsOpen] = useState(false)
  const [dropdownPosition, setDropdownPosition] = useState({ top: '0px', right: '0px' })

  useEffect(() => {
    const savedTheme = localStorage.getItem('colorTheme') || 'sol-green'
    setCurrentTheme(savedTheme)
    applyTheme(savedTheme)
  }, [])

  useEffect(() => {
    if (isOpen) {
      const button = document.getElementById('theme-switcher-button')
      if (button) {
        const rect = button.getBoundingClientRect()
        setDropdownPosition({
          top: `${rect.bottom + 12}px`,
          right: `${window.innerWidth - rect.right}px`
        })
      }
    }
  }, [isOpen])

  const applyTheme = (themeId: string) => {
    document.documentElement.setAttribute('data-color-theme', themeId)
    localStorage.setItem('colorTheme', themeId)
  }

  const handleThemeChange = (themeId: string) => {
    setCurrentTheme(themeId)
    applyTheme(themeId)
    setIsOpen(false)
  }

  return (
    <div className="relative">
      {/* Theme Switcher Button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="px-4 py-2 gap-2 bg-black electric-border text-[var(--theme-accent)] hover:bg-[var(--theme-bg-secondary)] transition-all duration-300 flex items-center font-mono-tech text-xs uppercase tracking-widest font-bold"
        aria-label="Change color theme"
        style={{ boxShadow: '0 0 15px var(--glow-color)' }}
        id="theme-switcher-button"
      >
        <span className="text-base">◉</span>
        <span className="hidden sm:inline">THEME</span>
      </button>

      {/* Dropdown Menu */}
      {isOpen && (
        <div 
          className="fixed w-72 bg-black border-2 border-[var(--theme-accent)] p-4"
          style={{ 
            boxShadow: '0 0 40px var(--glow-color-strong), inset 0 0 30px rgba(0,0,0,0.8)',
            zIndex: 10000,
            top: dropdownPosition.top,
            right: dropdownPosition.right
          }}
        >
          <div className="mb-4">
            <p className="font-display text-sm font-black text-[var(--theme-accent)] uppercase tracking-widest">
              [COLOR SCHEME]
            </p>
          </div>
          
          <div className="grid grid-cols-2 gap-3">
            {COLOR_THEMES.map((theme) => (
              <button
                key={theme.id}
                onClick={() => handleThemeChange(theme.id)}
                className={`
                  relative p-4 border-2 transition-all duration-200 bg-black
                  ${currentTheme === theme.id 
                    ? 'border-white' 
                    : 'border-transparent hover:border-gray-600'
                  }
                `}
                style={currentTheme === theme.id ? {
                  backgroundColor: theme.primaryColor + '15',
                  boxShadow: `0 0 20px ${theme.primaryColor}60, inset 0 0 20px rgba(0,0,0,0.5)`
                } : {}}
              >
                <div className="flex flex-col items-center gap-2">
                  <div 
                    className="w-10 h-10 flex items-center justify-center text-2xl font-bold"
                    style={{ 
                      backgroundColor: theme.primaryColor,
                      boxShadow: `0 0 20px ${theme.primaryColor}, inset 0 0 10px rgba(0,0,0,0.5)`,
                      color: '#000'
                    }}
                  >
                    {theme.icon}
                  </div>
                  <span className="font-mono-tech text-[10px] font-bold uppercase tracking-widest" style={{ color: theme.primaryColor }}>
                    {theme.name}
                  </span>
                  {currentTheme === theme.id && (
                    <div className="absolute top-2 right-2 w-2 h-2" style={{ backgroundColor: theme.primaryColor, boxShadow: `0 0 10px ${theme.primaryColor}` }}></div>
                  )}
                </div>
              </button>
            ))}
          </div>

          <div className="mt-4 pt-3 border-t-2 border-[var(--theme-accent)]/30">
            <p className="font-mono-tech text-[9px] text-[var(--theme-text-secondary)] text-center uppercase tracking-wider">
              // SYSTEM-WIDE COLOR OVERRIDE //
            </p>
          </div>
        </div>
      )}

      {/* Click outside to close */}
      {isOpen && (
        <div 
          className="fixed inset-0" 
          style={{ zIndex: 9999 }}
          onClick={() => setIsOpen(false)}
        />
      )}
    </div>
  )
}

export { COLOR_THEMES }
