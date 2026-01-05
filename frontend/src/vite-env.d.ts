/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_USE_WASM?: string
  readonly VITE_API_BASE_URL?: string
  readonly VITE_WS_URL?: string
  // Add other environment variables as needed
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}

// Webpack DefinePlugin globals
declare const __VITE_ENV__: {
  VITE_USE_WASM: string
  MODE: string
  DEV: boolean
  PROD: boolean
  BASE_URL: string
  VITE_API_BASE_URL: string
  VITE_WS_URL: string
}
