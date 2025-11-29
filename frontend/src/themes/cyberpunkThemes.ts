// flyonui Cyberpunk Theme Configuration
// Reduced from 7 themes to 3 cohesive themes

export interface CyberpunkTheme {
  id: string
  name: string
  primary: string
  secondary: string
  accent: string
  surface: string
  background: string
  text: {
    primary: string
    secondary: string
    muted: string
  }
  success: string
  warning: string
  error: string
  info: string
  glow: {
    color: string
    strong: string
  }
  glass: {
    bg: string
    border: string
  }
  border: {
    glow: string
  }
  palette?: string[] // multiple accent/brand colors for widgets
  animations?: 'none' | 'subtle' | 'full'
}

// 1. MATRIX GREEN - Cyberpunk Core Theme
export const MATRIX_THEME: CyberpunkTheme = {
  id: 'matrix',
  name: 'MATRIX',
  primary: '#00ff41',
  secondary: '#001a0a',
  accent: '#00ff41',
  surface: '#0f0f0f',
  background: '#000000',
  text: {
    primary: '#ffffff',
    secondary: '#b8ffcf',
    muted: '#5a7a66'
  },
  success: '#00ff41',
  warning: '#ffff00',
  error: '#ff0062',
  info: '#00d9ff',
  glow: {
    color: 'rgba(0, 255, 65, 0.6)',
    strong: 'rgba(0, 255, 65, 0.9)'
  },
  glass: {
    bg: 'rgba(0, 26, 10, 0.5)',
    border: 'rgba(0, 255, 65, 0.2)'
  },
  border: {
    glow: 'rgba(0, 255, 65, 0.3)'
  },
  palette: ['#00ff41', '#14F195', '#0fc273', '#0db16a', '#00e08a'],
  animations: 'full'
}

// 2. CYBER BLUE - Tech Modern Theme
export const CYBER_THEME: CyberpunkTheme = {
  id: 'cyber',
  name: 'CYBER',
  primary: '#00d9ff',
  secondary: '#00111f',
  accent: '#00d9ff',
  surface: '#00111f',
  background: '#000000',
  text: {
    primary: '#ffffff',
    secondary: '#7dd3fc',
    muted: '#1a4d66'
  },
  success: '#00ffcc',
  warning: '#ffaa00',
  error: '#ff4466',
  info: '#00d9ff',
  glow: {
    color: 'rgba(0, 217, 255, 0.6)',
    strong: 'rgba(0, 217, 255, 0.9)'
  },
  glass: {
    bg: 'rgba(0, 13, 26, 0.4)',
    border: 'rgba(0, 217, 255, 0.2)'
  },
  border: {
    glow: 'rgba(0, 217, 255, 0.3)'
  },
  palette: ['#00d9ff', '#4fe1ff', '#2ec7ff', '#00c3f6', '#00b7ef'],
  animations: 'full'
}

// 3. PLASMA PURPLE - Advanced Mode Theme
export const PLASMA_THEME: CyberpunkTheme = {
  id: 'plasma',
  name: 'PLASMA',
  primary: '#d946ef',
  secondary: '#14001f',
  accent: '#d946ef',
  surface: '#14001f',
  background: '#000000',
  text: {
    primary: '#ffffff',
    secondary: '#d8b4fe',
    muted: '#4d1a66'
  },
  success: '#a855f7',
  warning: '#f59e0b',
  error: '#ff006e',
  info: '#00d4ff',
  glow: {
    color: 'rgba(217, 70, 239, 0.6)',
    strong: 'rgba(217, 70, 239, 0.9)'
  },
  glass: {
    bg: 'rgba(15, 0, 26, 0.4)',
    border: 'rgba(217, 70, 239, 0.2)'
  },
  border: {
    glow: 'rgba(217, 70, 239, 0.3)'
  },
  palette: ['#d946ef', '#ef4be6', '#b84bf6', '#ff66f1', '#a755f7'],
  animations: 'full'
}

// Additional cyberpunk palettes
export const NEON_RED_THEME: CyberpunkTheme = {
  id: 'neon-red',
  name: 'NEON RED',
  primary: '#ff0055',
  secondary: '#1a0a0f',
  accent: '#ff0055',
  surface: '#10040a',
  background: '#040204',
  text: { primary: '#fff', secondary: '#ffb2c9', muted: '#6b2230' },
  success: '#00ff6a',
  warning: '#ffb400',
  error: '#ff0055',
  info: '#00d9ff',
  glow: { color: 'rgba(255, 0, 85, 0.5)', strong: 'rgba(255, 0, 85, 0.88)' },
  glass: { bg: 'rgba(20, 0, 8, 0.4)', border: 'rgba(255, 0, 85, 0.2)' },
  border: { glow: 'rgba(255, 0, 85, 0.3)' },
  palette: ['#ff0055', '#ff4f87', '#ff7aa1', '#ff9ab9', '#ffb0d0'],
  animations: 'full'
}

export const SYNTHWAVE_THEME: CyberpunkTheme = {
  id: 'synthwave',
  name: 'SYNTHWAVE',
  primary: '#ff77ff',
  secondary: '#100012',
  accent: '#ff77ff',
  surface: '#120013',
  background: '#020002',
  text: { primary: '#fff', secondary: '#ffd7ff', muted: '#441144' },
  success: '#50ffb4',
  warning: '#ffd166',
  error: '#ff3b6f',
  info: '#00d9ff',
  glow: { color: 'rgba(255, 119, 255, 0.55)', strong: 'rgba(255, 119, 255, 0.95)' },
  glass: { bg: 'rgba(20, 0, 20, 0.45)', border: 'rgba(255, 119, 255, 0.2)' },
  border: { glow: 'rgba(255, 119, 255, 0.28)' },
  palette: ['#ff77ff', '#ff4cff', '#ff00ff', '#ffb3ff', '#ff66ff'],
  animations: 'full'
}

// Export all themes
export const CYBERPUNK_THEMES = {
  matrix: MATRIX_THEME,
  cyber: CYBER_THEME,
  plasma: PLASMA_THEME
  , 'neon-red': NEON_RED_THEME,
  'synthwave': SYNTHWAVE_THEME
}

// Theme configuration for flyonui
export const FLYONUI_THEME_CONFIG = {
  themes: {
    matrix: {
      'primary': MATRIX_THEME.primary,
      'secondary': MATRIX_THEME.secondary,
      'accent': MATRIX_THEME.accent,
      'success': MATRIX_THEME.success,
      'warning': MATRIX_THEME.warning,
      'error': MATRIX_THEME.error,
      'info': MATRIX_THEME.info,
      'background': MATRIX_THEME.background,
      'surface': MATRIX_THEME.surface,
      'text-primary': MATRIX_THEME.text.primary,
      'text-secondary': MATRIX_THEME.text.secondary,
      'text-muted': MATRIX_THEME.text.muted,
    },
    cyber: {
      'primary': CYBER_THEME.primary,
      'secondary': CYBER_THEME.secondary,
      'accent': CYBER_THEME.accent,
      'success': CYBER_THEME.success,
      'warning': CYBER_THEME.warning,
      'error': CYBER_THEME.error,
      'info': CYBER_THEME.info,
      'background': CYBER_THEME.background,
      'surface': CYBER_THEME.surface,
      'text-primary': CYBER_THEME.text.primary,
      'text-secondary': CYBER_THEME.text.secondary,
      'text-muted': CYBER_THEME.text.muted,
    },
    plasma: {
      'primary': PLASMA_THEME.primary,
      'secondary': PLASMA_THEME.secondary,
      'accent': PLASMA_THEME.accent,
      'success': PLASMA_THEME.success,
      'warning': PLASMA_THEME.warning,
      'error': PLASMA_THEME.error,
      'info': PLASMA_THEME.info,
      'background': PLASMA_THEME.background,
      'surface': PLASMA_THEME.surface,
      'text-primary': PLASMA_THEME.text.primary,
      'text-secondary': PLASMA_THEME.text.secondary,
      'text-muted': PLASMA_THEME.text.muted,
    }
  }
}

// Theme utilities
export const applyCyberpunkTheme = (theme: CyberpunkTheme) => {
  const root = document.documentElement
  
  // Set CSS custom properties for the theme
  root.style.setProperty('--theme-bg-primary', theme.background)
  root.style.setProperty('--theme-bg-secondary', theme.surface)
  root.style.setProperty('--theme-bg-card', theme.surface)
  root.style.setProperty('--theme-bg-input', theme.secondary)
  root.style.setProperty('--theme-accent', theme.accent)
  root.style.setProperty('--theme-accent-hover', theme.primary)
  root.style.setProperty('--theme-accent-glow', theme.primary)
  root.style.setProperty('--theme-text-primary', theme.text.primary)
  root.style.setProperty('--theme-text-secondary', theme.text.secondary)
  root.style.setProperty('--theme-text-muted', theme.text.muted)
  root.style.setProperty('--theme-input-text', theme.text.primary)
  root.style.setProperty('--theme-input-border', theme.primary)
  root.style.setProperty('--theme-button-bg', theme.secondary)
  root.style.setProperty('--theme-button-text', theme.primary)
  theme.text.primary // Reference to avoid unused variable warning
  
  root.style.setProperty('--theme-success', theme.success)
  root.style.setProperty('--theme-warning', theme.warning)
  root.style.setProperty('--theme-error', theme.error)
  root.style.setProperty('--theme-info', theme.info)
  root.style.setProperty('--glow-color', theme.glow.color)
  root.style.setProperty('--glow-color-strong', theme.glow.strong)
  root.style.setProperty('--border-glow', theme.border.glow)
  root.style.setProperty('--glass-bg', theme.glass.bg)
  root.style.setProperty('--glass-border', theme.glass.border)
  // Create palette CSS variables
  if (theme.palette && theme.palette.length > 0) {
    theme.palette.forEach((col, idx) => {
      root.style.setProperty(`--brand-${idx + 1}`, col)
      root.style.setProperty(`--brand-${idx + 1}-muted`, col + '66')
    })
  }
  
  // Set data attribute for theme switching
  root.setAttribute('data-color-theme', theme.id)
  // Also set data-theme for DaisyUI/FlyonUI compatibility
  root.setAttribute('data-theme', theme.id)
  // Set animation preference as attribute
  root.setAttribute('data-animations', theme.animations || 'full')
  
  // Store in localStorage
  localStorage.setItem('cyberpunkTheme', theme.id)
}

// Get theme by ID
export const getCyberpunkTheme = (themeId: string): CyberpunkTheme => {
  switch (themeId) {
    case 'matrix':
      return MATRIX_THEME
    case 'cyber':
      return CYBER_THEME
    case 'plasma':
      return PLASMA_THEME
    default:
      return MATRIX_THEME
  }
}

// Initialize theme from localStorage
export const initializeCyberpunkTheme = (): CyberpunkTheme => {
  const savedTheme = localStorage.getItem('cyberpunkTheme') || 'matrix'
  const theme = getCyberpunkTheme(savedTheme)
  applyCyberpunkTheme(theme)
  return theme
}