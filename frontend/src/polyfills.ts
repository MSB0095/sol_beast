// Polyfills for Solana libraries
// Import buffer polyfill and make it globally available
import { Buffer } from 'buffer/'

// Ensure Buffer is available globally before any other modules load
;(window as any).Buffer = Buffer
;(globalThis as any).Buffer = Buffer
;(window as any).global = window

console.log('Polyfills loaded, Buffer available:', typeof Buffer !== 'undefined')
