// Polyfills for Solana libraries
import { Buffer } from 'buffer'

// Make Buffer available globally
(window as any).Buffer = Buffer;
(globalThis as any).Buffer = Buffer;
