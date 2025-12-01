import { useEffect, useState } from 'react';
import { useWallet } from '@solana/wallet-adapter-react';
import { useWalletStore } from '../store/walletStore';
import { Wallet, Key, Shield, Copy, Check, AlertCircle } from 'lucide-react';

export default function ProfilePanel() {
  const { publicKey, signMessage, connected } = useWallet();
  const { associatedWallet, loading, error, loadAssociatedWallet, createAssociatedWallet, clearAssociatedWallet } = useWalletStore();
  const [copied, setCopied] = useState(false);
  const [creating, setCreating] = useState(false);
  const [showPrivateKey, setShowPrivateKey] = useState(false);

  useEffect(() => {
    if (publicKey) {
      loadAssociatedWallet(publicKey.toBase58());
    } else {
      clearAssociatedWallet();
    }
  }, [publicKey, loadAssociatedWallet, clearAssociatedWallet]);

  const handleCreateAssociatedWallet = async () => {
    if (!publicKey || !signMessage) {
      return;
    }

    setCreating(true);
    try {
      // Create message to sign
      const message = new TextEncoder().encode(
        `Sign this message to create your Sol Beast trading wallet.\n\nTimestamp: ${Date.now()}\nWallet: ${publicKey.toBase58()}`
      );

      // Request signature from user
      const signature = await signMessage(message);

      // Create associated wallet
      await createAssociatedWallet(publicKey.toBase58(), signature);
    } catch (err) {
      console.error('Failed to create associated wallet:', err);
    } finally {
      setCreating(false);
    }
  };

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  if (!connected) {
    return (
      <div className="space-y-6">
        <div className="cyber-card p-8 text-center">
          <Wallet size={64} className="mx-auto mb-4 opacity-50" style={{ color: 'var(--theme-text-muted)' }} />
          <h3 className="font-display text-2xl font-black mb-3 uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>
            Connect Your Wallet
          </h3>
          <p className="text-sm mb-6" style={{ color: 'var(--theme-text-muted)' }}>
            Connect your Solana wallet to access trading features and manage your profile.
          </p>
          <p className="text-xs font-mono-tech" style={{ color: 'var(--theme-text-muted)' }}>
            Use the wallet button in the top right corner to connect.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Connected Wallet Info */}
      <div className="cyber-card p-6">
        <h3 className="font-display text-lg font-black mb-5 glow-text uppercase tracking-wider flex items-center gap-2">
          <Wallet size={20} />
          Connected Wallet
        </h3>
        <div className="glass-card p-4 rounded-xl">
          <p className="text-xs mb-2 uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>
            Public Address
          </p>
          <div className="flex items-center justify-between gap-3">
            <p className="font-mono-tech text-sm truncate" style={{ color: 'var(--theme-accent)' }}>
              {publicKey?.toBase58()}
            </p>
            <button
              onClick={() => copyToClipboard(publicKey?.toBase58() || '')}
              className="p-2 rounded-lg hover:bg-black/30 transition-all"
            >
              {copied ? <Check size={16} /> : <Copy size={16} />}
            </button>
          </div>
        </div>
      </div>

      {/* Associated Trading Wallet */}
      <div className="cyber-card p-6">
        <h3 className="font-display text-lg font-black mb-5 glow-text uppercase tracking-wider flex items-center gap-2">
          <Key size={20} />
          Trading Wallet
        </h3>

        {error && (
          <div className="alert-error rounded-xl p-4 mb-4 flex gap-3">
            <AlertCircle size={20} className="flex-shrink-0 mt-0.5" />
            <div>
              <p className="font-bold text-sm">Error</p>
              <p className="text-xs opacity-90">{error}</p>
            </div>
          </div>
        )}

        {!associatedWallet ? (
          <div className="glass-card p-6 rounded-xl text-center">
            <Shield size={48} className="mx-auto mb-4 opacity-50" style={{ color: 'var(--theme-accent)' }} />
            <p className="text-sm mb-4" style={{ color: 'var(--theme-text-secondary)' }}>
              Create a dedicated trading wallet associated with your connected wallet.
              This wallet will be used for automated trading operations.
            </p>
            <button
              onClick={handleCreateAssociatedWallet}
              disabled={creating || loading}
              className="btn-primary px-6 py-3 rounded-xl font-mono-tech text-sm uppercase tracking-wider disabled:opacity-50"
            >
              {creating ? 'Creating...' : 'Create Trading Wallet'}
            </button>
            <p className="text-xs mt-4 font-mono-tech" style={{ color: 'var(--theme-text-muted)' }}>
              You'll be asked to sign a message to verify ownership.
            </p>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="glass-card p-4 rounded-xl">
              <p className="text-xs mb-2 uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>
                Trading Wallet Address
              </p>
              <div className="flex items-center justify-between gap-3">
                <p className="font-mono-tech text-sm truncate" style={{ color: 'var(--theme-accent)' }}>
                  {associatedWallet.publicKey}
                </p>
                <button
                  onClick={() => copyToClipboard(associatedWallet.publicKey)}
                  className="p-2 rounded-lg hover:bg-black/30 transition-all"
                >
                  {copied ? <Check size={16} /> : <Copy size={16} />}
                </button>
              </div>
            </div>

            <div className="glass-card p-4 rounded-xl">
              <div className="flex items-center justify-between mb-2">
                <p className="text-xs uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>
                  Private Key (Keep Secure!)
                </p>
                <button
                  onClick={() => setShowPrivateKey(!showPrivateKey)}
                  className="text-xs font-mono-tech uppercase tracking-wider px-3 py-1 rounded-lg hover:bg-black/30 transition-all"
                  style={{ color: 'var(--theme-accent)' }}
                >
                  {showPrivateKey ? 'Hide' : 'Show'}
                </button>
              </div>
              {showPrivateKey ? (
                <div className="flex items-center justify-between gap-3 bg-black/50 p-3 rounded-lg">
                  <p className="font-mono text-xs break-all" style={{ color: 'var(--theme-warning)' }}>
                    {associatedWallet.privateKey}
                  </p>
                  <button
                    onClick={() => copyToClipboard(associatedWallet.privateKey)}
                    className="p-2 rounded-lg hover:bg-black/30 transition-all flex-shrink-0"
                  >
                    {copied ? <Check size={16} /> : <Copy size={16} />}
                  </button>
                </div>
              ) : (
                <p className="font-mono text-sm" style={{ color: 'var(--theme-text-muted)' }}>
                  ••••••••••••••••••••••••••••••••
                </p>
              )}
            </div>

            <div className="alert-warning p-4 rounded-xl">
              <p className="text-xs font-bold mb-2 uppercase tracking-wider">Security Notice</p>
              <ul className="text-xs space-y-1" style={{ color: 'var(--theme-text-secondary)' }}>
                <li>• Only you can see this private key</li>
                <li>• Never share it with anyone</li>
                <li>• Store it safely as a backup</li>
                <li>• This wallet is derived from your signature</li>
              </ul>
            </div>

            <div className="glass-card p-4 rounded-xl">
              <p className="text-xs mb-2 uppercase tracking-wider" style={{ color: 'var(--theme-text-secondary)' }}>
                Created At
              </p>
              <p className="font-mono-tech text-sm" style={{ color: 'var(--theme-accent)' }}>
                {new Date(associatedWallet.createdAt).toLocaleString()}
              </p>
            </div>
          </div>
        )}
      </div>

      {/* Instructions */}
      <div className="cyber-card p-6">
        <h3 className="font-display text-lg font-black mb-4 glow-text uppercase tracking-wider">
          How It Works
        </h3>
        <div className="space-y-3 text-sm font-mono-tech" style={{ color: 'var(--theme-text-secondary)' }}>
          <p>1. Connect your Solana wallet (Phantom, Solflare, etc.)</p>
          <p>2. Sign a message to create your trading wallet</p>
          <p>3. Your trading wallet is generated deterministically from your signature</p>
          <p>4. The bot uses this wallet for automated trading</p>
          <p>5. Only you can access your trading wallet with your connected wallet</p>
        </div>
      </div>
    </div>
  );
}
