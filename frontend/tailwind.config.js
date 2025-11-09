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
      },
    },
  },
  plugins: [],
}
