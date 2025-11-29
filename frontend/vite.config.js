import { defineConfig } from 'vite';
import * as path from 'path';
import react from '@vitejs/plugin-react';
import { nodePolyfills } from 'vite-plugin-node-polyfills';
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import inject from '@rollup/plugin-inject';
// https://vitejs.dev/config/
export default defineConfig({
    plugins: [
        // Inject Buffer globally in both dev and build so env modules do not throw
        // when they reference `Buffer` before our runtime shims load.
        nodePolyfills(),
        inject({ Buffer: ['buffer', 'Buffer'] }),
        react(),
        wasm(),
        topLevelAwait()
    ],
    define: {
        // Make `global` available for libraries that expect Node's global
        global: 'globalThis',
    },
    resolve: {
        alias: {
            // Ensure `buffer` imports resolve to the browser shim
            buffer: 'buffer',
            'borsh': path.resolve(__dirname, 'src/shims/borsh-compat.js')
        }
    },
    optimizeDeps: {
        include: ['buffer', 'borsh', '@solana/web3.js']
    },
    base: process.env.VITE_BASE || (process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/'),
    build: {
        outDir: 'dist',
        assetsDir: 'assets',
        sourcemap: false,
        rollupOptions: {
            plugins: [
                // Inject Buffer import wherever `Buffer` is used so it exists during module evaluation
                // (Retained for rollup/production bundles)
                inject({ Buffer: ['buffer', 'Buffer'] })
            ]
        },
        commonjsOptions: {
            transformMixedEsModules: true,
            include: [/node_modules/] // Ensure CJS dependencies like borsh are transformed correctly for ESM imports
        }
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
