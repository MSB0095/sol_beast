/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        'sol-purple': '#14F195',
        'sol-dark': '#0B0E11',
        'sol-darker': '#080A0D',
        'sol-accent': '#14F195',
        'sol-accent-hover': '#10d882',
        'sol-blue': '#3B82F6',
        'sol-indigo': '#6366F1',
        'sol-violet': '#8B5CF6',
        'sol-cyan': '#06B6D4',
      },
      backgroundImage: {
        'gradient-radial': 'radial-gradient(var(--tw-gradient-stops))',
        'gradient-conic': 'conic-gradient(from 180deg at 50% 50%, var(--tw-gradient-stops))',
        'glass': 'linear-gradient(135deg, rgba(255, 255, 255, 0.05) 0%, rgba(255, 255, 255, 0.02) 100%)',
      },
      boxShadow: {
        'glow': '0 0 20px rgba(20, 241, 149, 0.3)',
        'glow-lg': '0 0 30px rgba(20, 241, 149, 0.4)',
        'glass': '0 8px 32px 0 rgba(31, 38, 135, 0.37)',
        'card': '0 4px 6px -1px rgba(0, 0, 0, 0.3), 0 2px 4px -1px rgba(0, 0, 0, 0.24)',
        'card-hover': '0 10px 20px -5px rgba(0, 0, 0, 0.4), 0 4px 8px -2px rgba(0, 0, 0, 0.3)',
      },
      backdropBlur: {
        'glass': '12px',
      },
    },
  },
  plugins: [],
}
