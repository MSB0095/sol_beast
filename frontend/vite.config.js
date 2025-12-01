import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { writeFileSync } from 'fs';
import { resolve } from 'path';
import { nodePolyfills } from 'vite-plugin-node-polyfills';
// Plugin to create .nojekyll file for GitHub Pages
function createNoJekyllPlugin() {
    return {
        name: 'create-nojekyll',
        closeBundle: function () {
            var outDir = resolve(__dirname, 'dist');
            writeFileSync(resolve(outDir, '.nojekyll'), '');
        }
    };
}
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
    base: process.env.NODE_ENV === 'production' ? '/sol_beast/' : '/',
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
                rewrite: function (path) { return path.replace(/^\/api/, ''); }
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
});
