import React from 'react'
// Polyfill Node Buffer in browser for libs that rely on Buffer (e.g., wasm helpers)
import { Buffer } from 'buffer'
;(window as any).Buffer = Buffer
import ReactDOM from 'react-dom/client'
import App from './App.tsx'
import './index.css'
import { SolanaWalletProvider } from './contexts/WalletProvider'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <SolanaWalletProvider>
      <App />
    </SolanaWalletProvider>
  </React.StrictMode>,
)
