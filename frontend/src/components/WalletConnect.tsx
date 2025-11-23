import { useWallet } from '@solana/wallet-adapter-react'
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import { useEffect } from 'react'
import { useUserSessionStore } from '../store/userSessionStore'

export function WalletConnect() {
  const { publicKey, connected, disconnect } = useWallet()
  const { setWallet, getCurrentUserSettings, clearSession } = useUserSessionStore()

  useEffect(() => {
    if (connected && publicKey) {
      setWallet(publicKey)
      const settings = getCurrentUserSettings()
      console.log('Wallet connected:', publicKey.toString())
      console.log('User settings loaded:', settings)
    } else {
      clearSession()
    }
  }, [connected, publicKey, setWallet, getCurrentUserSettings, clearSession])

  return (
    <div className="wallet-connect-container">
      <WalletMultiButton className="wallet-adapter-button-custom" />
      
      {connected && publicKey && (
        <div className="mt-4 p-4 bg-[var(--theme-bg-card)] electric-border">
          <div className="flex items-center justify-between">
            <div>
              <p className="font-mono-tech text-xs text-[var(--theme-text-secondary)] uppercase tracking-wider mb-1">
                Connected Wallet
              </p>
              <p className="font-mono-tech text-sm text-[var(--theme-accent)] font-bold">
                {publicKey.toString().slice(0, 8)}...{publicKey.toString().slice(-8)}
              </p>
            </div>
            <button
              onClick={disconnect}
              className="btn btn-sm btn-outline text-xs uppercase tracking-wider"
            >
              Disconnect
            </button>
          </div>
          
          <div className="mt-3 pt-3 border-t border-[var(--theme-border)]">
            <p className="font-mono-tech text-xs text-[var(--theme-text-secondary)]">
              🔐 Your settings are automatically saved and restored for this wallet
            </p>
          </div>
        </div>
      )}
      
      {!connected && (
        <div className="mt-4 p-4 bg-[var(--theme-bg-card)] electric-border">
          <p className="font-mono-tech text-sm text-[var(--theme-text-secondary)] mb-2">
            Connect your Solana wallet to start trading
          </p>
          <ul className="font-mono-tech text-xs text-[var(--theme-text-secondary)] space-y-1">
            <li>✓ Phantom</li>
            <li>✓ Solflare</li>
            <li>✓ Coinbase Wallet</li>
            <li>✓ Ledger</li>
          </ul>
        </div>
      )}
    </div>
  )
}
