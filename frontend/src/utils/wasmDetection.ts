/**
 * Utility function to detect if the application is running in WASM mode
 * WASM mode is enabled when:
 * 1. The botService is already initialized in WASM mode, OR
 * 2. VITE_USE_WASM environment variable is set to 'true', OR
 * 3. The application is hosted on GitHub Pages (hostname ends with .github.io)
 */
export function isWasmMode(botService?: { isWasmMode: () => boolean }): boolean {
  // Check if botService is provided and already in WASM mode
  if (botService && botService.isWasmMode()) {
    return true
  }

  // Check environment variable
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  if (typeof window !== 'undefined' && (import.meta as any).env?.VITE_USE_WASM === 'true') {
    return true
  }

  // Check if running on GitHub Pages
  if (typeof window !== 'undefined' && window.location.hostname.endsWith('.github.io')) {
    return true
  }

  return false
}
