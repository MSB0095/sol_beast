import { useState, useEffect } from 'react'
import { applyCyberpunkTheme, getCyberpunkTheme, CYBERPUNK_THEMES, type CyberpunkTheme } from '../themes/cyberpunkThemes'

// flyonui-based theme switcher with 3 themes
export function ThemeSwitcher() {
  const [currentTheme, setCurrentTheme] = useState<CyberpunkTheme>(getCyberpunkTheme('matrix'))

  useEffect(() => {
    const savedThemeId = localStorage.getItem('cyberpunkTheme') || 'matrix'
    const theme = getCyberpunkTheme(savedThemeId)
    setCurrentTheme(theme)
  }, [])

  const handleThemeChange = (themeId: string) => {
    const theme = getCyberpunkTheme(themeId)
    setCurrentTheme(theme)
    applyCyberpunkTheme(theme)
    setAnimationsOn(theme.animations !== 'none')
    // Close action handled by dropdown semantics; no explicit state needed here
  }

  const themes = Object.values(CYBERPUNK_THEMES)
  const [animationsOn, setAnimationsOn] = useState<boolean>(getCyberpunkTheme(localStorage.getItem('cyberpunkTheme') || 'matrix').animations !== 'none')

  return (
    <div className="dropdown dropdown-end">
      <button tabIndex={0} className="btn btn-ghost btn-sm gap-2 border" aria-label="Change color theme">
        <span className="w-4 h-4 rounded-full border-2" style={{ backgroundColor: currentTheme.primary, boxShadow: `0 0 10px ${currentTheme.primary}`, borderColor: currentTheme.primary }} />
        <span className="hidden sm:inline font-mono">THEME</span>
      </button>
      
      <div tabIndex={0} className="dropdown-content z-[1] card card-compact w-56 p-3 glass-card bg-black border-2 border-[var(--theme-accent)]">
        <div className="card-body">
          <h3 className="font-display text-sm font-black text-primary uppercase tracking-wider mb-2">
            Color Scheme
          </h3>
          
          <div className="grid grid-cols-3 gap-2">
            {themes.map((theme) => (
              <button
                key={theme.id}
                onClick={() => handleThemeChange(theme.id)}
                className={`p-2 rounded-md transition-all duration-200 border ${currentTheme.id === theme.id ? 'border-white' : 'border-transparent hover:border-primary/30'}`}                
                style={{ background: `linear-gradient(135deg, ${theme.palette?.[0] || theme.primary}, ${theme.palette?.[1] || theme.primary}88)` }}
                aria-label={`apply-${theme.name}`}
              >
                <div className="h-10 w-10 rounded-md flex items-center justify-center text-xs font-bold text-black" style={{ backgroundColor: theme.palette?.[0] || theme.primary, boxShadow: `0 0 8px ${theme.palette?.[0] || theme.primary}` }}>
                </div>
              </button>
            ))}
          </div>

          <div className="mt-3 pt-3 border-t border-[var(--theme-accent)]/20 flex items-center justify-between">
            <p className="font-mono text-[9px] text-[var(--theme-text-secondary)] uppercase tracking-wider">CYBERPUNK THEMES</p>
            <div className="flex items-center gap-2">
              <label className="label-text text-sm">Buzzy</label>
              <input type="checkbox" checked={animationsOn} onChange={(e) => { setAnimationsOn(e.target.checked); document.documentElement.setAttribute('data-animations', e.target.checked ? 'full' : 'none') }} className="toggle toggle-sm" />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

export { CYBERPUNK_THEMES }
