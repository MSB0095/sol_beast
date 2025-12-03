import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { writeFileSync } from 'fs'
import { resolve } from 'path'
import { nodePolyfills } from 'vite-plugin-node-polyfills'

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

// Allow base path to be configured via environment variable for easy deployment to different repositories
const BASE_PATH = process.env.BASE_PATH || '/sol_beast/'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(),
    nodePolyfills({
      // Enable all Node.js polyfills
      include: ['buffer', 'process', 'util', 'stream', 'events'],
      globals: {
        Buffer: true,
        global: true,
        process: true,
      },
    }),
    createNoJekyllPlugin(),
  ],
  base: process.env.NODE_ENV === 'production' ? BASE_PATH : '/',
  define: {
    'global': 'globalThis',
  },
  resolve: {
    alias: {
      buffer: 'buffer/',
      process: 'process/browser',
      util: 'util/',
    },
  },
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
    commonjsOptions: {
      transformMixedEsModules: true,
    },
    rollupOptions: {
      output: {
        // Let Vite automatically handle chunking to avoid circular dependency issues
        // Manual chunking of @solana/web3.js causes "Cannot access before initialization" errors
        manualChunks: undefined
      }
    }
  }
})
