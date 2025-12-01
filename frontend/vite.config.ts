import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { writeFileSync } from 'fs'
import { resolve } from 'path'

// Plugin to create .nojekyll file for GitHub Pages
function createNoJekyllPlugin() {
  return {
    name: 'create-nojekyll',
    closeBundle() {
      const outDir = resolve(__dirname, 'dist')
      writeFileSync(resolve(outDir, '.nojekyll'), '')
    }
  }
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react(), createNoJekyllPlugin()],
  base: process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/',
  server: {
    port: 3000,
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api/, '')
      }
    }
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks: {
          'wallet-adapter': [
            '@solana/wallet-adapter-base',
            '@solana/wallet-adapter-react',
            '@solana/wallet-adapter-react-ui',
            '@solana/wallet-adapter-wallets',
          ],
          'solana-web3': ['@solana/web3.js'],
        }
      }
    }
  }
})
