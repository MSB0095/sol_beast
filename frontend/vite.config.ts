import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import { writeFileSync } from 'fs'
import { resolve } from 'path'
import { NodeGlobalsPolyfillPlugin } from '@esbuild-plugins/node-globals-polyfill'
import { NodeModulesPolyfillPlugin } from '@esbuild-plugins/node-modules-polyfill'
import inject from '@rollup/plugin-inject'

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

// Plugin to inject polyfills script into HTML before main module
function injectBufferPolyfillPlugin() {
  return {
    name: 'inject-buffer-polyfill',
    transformIndexHtml(html: string) {
      // Polyfills will be auto-injected as a separate entry point
      // Just ensure global is set
      const polyfillScript = `<script>window.global = window;</script>\n    `
      return html.replace(/<script/, polyfillScript + '<script')
    }
  }
}

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [
    react(), 
    createNoJekyllPlugin(),
    injectBufferPolyfillPlugin(),
  ],
  base: process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/',
  define: {
    'global': 'globalThis',
    'process.env': '{}',
  },
  optimizeDeps: {
    esbuildOptions: {
      define: {
        global: 'globalThis'
      },
      plugins: [
        NodeGlobalsPolyfillPlugin({
          buffer: true,
        }),
        NodeModulesPolyfillPlugin(),
      ],
    },
  },
  resolve: {
    conditions: ['browser', 'module', 'import', 'default'],
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
