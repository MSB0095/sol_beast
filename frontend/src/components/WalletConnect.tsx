import { useWallet } from '@solana/wallet-adapter-react';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { useEffect, useState } from 'react';
import { useWasmStore } from '../store/wasmStore';
import { Wallet, AlertCircle, CheckCircle, User, TrendingUp, Activity } from 'lucide-react';

export default function WalletConnect() {
  const { publicKey, connected } = useWallet();
  const { bot, initialized, connectWallet, disconnectWallet } = useWasmStore();
  const [userAccount, setUserAccount] = useState<Record<string, unknown> | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (connected && publicKey && initialized && bot) {
      // Connect wallet to WASM bot
      const address = publicKey.toBase58();
      connectWallet(address)
        .then((account) => {
          setUserAccount(account as Record<string, unknown> | null);
          setError(null);
        })
        .catch((err) => {
          console.error('Failed to connect wallet to bot:', err);
          setError('Failed to load user account');
        });
    } else if (!connected) {
      disconnectWallet();
      setUserAccount(null);
    }
  }, [connected, publicKey, initialized, bot, connectWallet, disconnectWallet]);

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-3">
        <div className="p-3 bg-primary/10 rounded-lg">
          <Wallet className="w-6 h-6 text-primary" />
        </div>
        <div>
          <h3 className="text-xl font-bold text-base-content uppercase tracking-wider">
            Wallet Connection
          </h3>
          <p className="text-base-content/60">Connect your Solana wallet to start trading</p>
        </div>
      </div>

      {/* Wallet Connection Section */}
      <div className="card bg-base-200/50 border border-base-300 rounded-xl">
        <div className="card-body">
          <div className="flex flex-col gap-4">
            {/* Wallet Button */}
            <div className="flex items-center justify-center">
              <WalletMultiButton className="btn btn-primary btn-wide" />
            </div>

            {/* Connection Status */}
            {connected && (
              <div className="flex items-center justify-center gap-3 p-3 bg-base-100 rounded-lg">
                {userAccount ? (
                  <>
                    <CheckCircle className="w-5 h-5 text-success" />
                    <span className="font-semibold text-success">Account Loaded</span>
                  </>
                ) : (
                  <>
                    <Wallet className="w-5 h-5 text-warning animate-pulse" />
                    <span className="font-semibold text-warning">Loading Account...</span>
                  </>
                )}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Error Alert */}
      {error && (
        <div role="alert" className="alert alert-error">
          <AlertCircle className="w-5 h-5" />
          <div>
            <h3 className="font-bold uppercase tracking-wider">Connection Error</h3>
            <div className="text-xs">{error}</div>
          </div>
        </div>
      )}

      {/* Account Information */}
      {userAccount && connected && (
        <div className="card bg-base-200/50 border border-success/20 rounded-xl">
          <div className="card-body">
            <div className="flex items-center gap-3 mb-4">
              <User className="w-5 h-5 text-success" />
              <h4 className="card-title text-base font-bold uppercase tracking-wider">
                Account Information
              </h4>
            </div>
            
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {/* Wallet Address */}
              <div className="bg-base-100 rounded-lg p-3">
                <div className="flex items-center gap-2 mb-1">
                  <Wallet className="w-4 h-4 text-base-content/60" />
                  <span className="text-sm font-medium text-base-content/60 uppercase">Wallet Address</span>
                </div>
                <code className="text-sm font-mono text-primary break-all">
                  {publicKey?.toBase58()}
                </code>
              </div>

              {/* Total Trades */}
              <div className="bg-base-100 rounded-lg p-3">
                <div className="flex items-center gap-2 mb-1">
                  <Activity className="w-4 h-4 text-base-content/60" />
                  <span className="text-sm font-medium text-base-content/60 uppercase">Total Trades</span>
                </div>
                  <span className="text-lg font-bold text-primary">
                  {typeof userAccount?.['total_trades'] === 'number' ? (userAccount!['total_trades'] as number) : 0}
                </span>
              </div>

              {/* Total P/L */}
              <div className="bg-base-100 rounded-lg p-3 md:col-span-2">
                <div className="flex items-center gap-2 mb-1">
                  <TrendingUp className="w-4 h-4 text-base-content/60" />
                  <span className="text-sm font-medium text-base-content/60 uppercase">Total P/L</span>
                </div>
                <span className={`text-xl font-bold ${
                  (typeof userAccount?.['total_profit_loss'] === 'number' ? (userAccount!['total_profit_loss'] as number) : 0) >= 0 ? 'text-success' : 'text-error'
                }`}>
                  {(typeof userAccount?.['total_profit_loss'] === 'number' && (userAccount!['total_profit_loss'] as number) >= 0) ? '+' : ''}
                  {typeof userAccount?.['total_profit_loss'] === 'number' ? (userAccount!['total_profit_loss'] as number).toFixed(4) : '0.0000'} SOL
                </span>
              </div>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
