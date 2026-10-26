import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  base: '/',
  server: {
    port: 3000,
    proxy: {
      '/v1': {
        target: 'http://127.0.0.1:8081',
        changeOrigin: true,
      },
      '/healthz': {
        target: 'http://127.0.0.1:8081',
        changeOrigin: true,
      },
      '/readyz': {
        target: 'http://127.0.0.1:8081',
        changeOrigin: true,
      },
    }
  },
  build: {
    outDir: 'dist',
    sourcemap: false,
    rollupOptions: {
      output: {
        manualChunks: {
          react: ['react', 'react-dom'],
          charts: ['recharts'],
          icons: ['lucide-react'],
        }
      }
    }
  }
})
