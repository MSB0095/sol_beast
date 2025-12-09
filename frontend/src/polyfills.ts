// Polyfills for Solana libraries
// Import buffer polyfill and make it globally available
import { Buffer } from 'buffer/'

// Ensure Buffer is available globally before any other modules load
;(window as unknown as { Buffer: typeof Buffer }).Buffer = Buffer
;(globalThis as unknown as { Buffer: typeof Buffer }).Buffer = Buffer
;(window as unknown as { global: Window }).global = window

console.log('Polyfills loaded, Buffer available:', typeof Buffer !== 'undefined')
