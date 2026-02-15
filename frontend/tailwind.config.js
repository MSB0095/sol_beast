/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      fontFamily: {
        'display': ['Orbitron', 'sans-serif'],
        'mono-tech': ['JetBrains Mono', 'monospace'],
        'ui': ['Rajdhani', 'sans-serif'],
        'share': ['Share Tech Mono', 'monospace'],
      },
      backgroundImage: {
        'gradient-radial': 'radial-gradient(var(--tw-gradient-stops))',
        'gradient-conic': 'conic-gradient(from 180deg at 50% 50%, var(--tw-gradient-stops))',
        'glass': 'linear-gradient(135deg, rgba(255, 255, 255, 0.05) 0%, rgba(255, 255, 255, 0.02) 100%)',
      },
      boxShadow: {
        'glow': '0 0 20px var(--glow-color)',
        'glow-lg': '0 0 30px var(--glow-color)',
        'glow-xl': '0 0 50px var(--glow-color-strong)',
        'glass': '0 8px 32px 0 rgba(31, 38, 135, 0.37)',
        'card': '0 4px 6px -1px rgba(0, 0, 0, 0.3), 0 2px 4px -1px rgba(0, 0, 0, 0.24)',
        'card-hover': '0 10px 20px -5px rgba(0, 0, 0, 0.4), 0 4px 8px -2px rgba(0, 0, 0, 0.3)',
        'profit': '0 0 30px rgba(0, 255, 65, 0.6), 0 0 60px rgba(0, 255, 65, 0.3)',
      },
      backdropBlur: {
        'glass': '12px',
      },
      animation: {
        'gradient': 'gradient 15s ease infinite',
        'profit-pulse': 'profit-pulse 2s ease-in-out infinite',
        'profit-celebrate': 'profit-celebrate 0.6s ease-out forwards',
        'counter-up': 'counter-up 0.8s cubic-bezier(0.16, 1, 0.3, 1) forwards',
        'sparkle': 'sparkle 1.5s ease-in-out infinite',
        'border-flow': 'border-flow 3s linear infinite',
        'data-stream': 'data-stream 20s linear infinite',
        'glow-breathe': 'glow-breathe 4s ease-in-out infinite',
      },
      keyframes: {
        gradient: {
          '0%, 100%': { backgroundPosition: '0% 50%' },
          '50%': { backgroundPosition: '100% 50%' },
        },
        'profit-pulse': {
          '0%, 100%': { boxShadow: '0 0 20px rgba(0, 255, 65, 0.4), inset 0 0 20px rgba(0, 255, 65, 0.1)' },
          '50%': { boxShadow: '0 0 40px rgba(0, 255, 65, 0.8), inset 0 0 40px rgba(0, 255, 65, 0.2)' },
        },
        'profit-celebrate': {
          '0%': { transform: 'scale(1)', opacity: '1' },
          '50%': { transform: 'scale(1.08)', opacity: '1' },
          '100%': { transform: 'scale(1)', opacity: '1' },
        },
        'counter-up': {
          '0%': { transform: 'translateY(20px)', opacity: '0' },
          '100%': { transform: 'translateY(0)', opacity: '1' },
        },
        'sparkle': {
          '0%, 100%': { opacity: '0', transform: 'scale(0) rotate(0deg)' },
          '50%': { opacity: '1', transform: 'scale(1) rotate(180deg)' },
        },
        'border-flow': {
          '0%': { backgroundPosition: '0% 0%' },
          '100%': { backgroundPosition: '200% 0%' },
        },
        'data-stream': {
          '0%': { transform: 'translateY(0)' },
          '100%': { transform: 'translateY(-50%)' },
        },
        'glow-breathe': {
          '0%, 100%': { filter: 'drop-shadow(0 0 8px var(--glow-color))' },
          '50%': { filter: 'drop-shadow(0 0 20px var(--glow-color-strong))' },
        },
      },
    },
  },
  plugins: [],
}
