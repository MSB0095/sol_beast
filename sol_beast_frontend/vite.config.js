import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { nodePolyfills } from 'vite-plugin-node-polyfills';
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
// https://vitejs.dev/config/
export default defineConfig({
    plugins: [
        nodePolyfills(),
        react(),
        wasm(),
        topLevelAwait()
    ],
    define: {
        'global.Buffer': 'Buffer'
    },
    base: process.env.VITE_BASE || (process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/'),
    build: {
        outDir: 'dist',
        assetsDir: 'assets',
        sourcemap: false,
    },
    server: {
        port: 3000,
        proxy: {
            '/api': {
                target: 'http://localhost:8080',
                changeOrigin: true,
                rewrite: function (path) { return path.replace(/^\/api/, ''); }
            }
        }
    }
});
