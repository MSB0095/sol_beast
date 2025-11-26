import { useWallet } from '@solana/wallet-adapter-react';
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui';
import { useEffect, useState } from 'react';
import { useWasmStore } from '../store/wasmStore';
import { Wallet, AlertCircle, CheckCircle } from 'lucide-react';

export default function WalletConnect() {
  const { publicKey, connected } = useWallet();
  const { bot, initialized, connectWallet, disconnectWallet } = useWasmStore();
  const [userAccount, setUserAccount] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (connected && publicKey && initialized && bot) {
      // Connect wallet to WASM bot
      const address = publicKey.toBase58();
      connectWallet(address)
        .then((account) => {
          setUserAccount(account);
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
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <WalletMultiButton className="btn btn-primary" />
        
        {connected && (
          <div className="flex items-center gap-2 px-3 py-2 bg-base-200 rounded-lg">
            {userAccount ? (
              <>
                <CheckCircle className="w-4 h-4 text-success" />
                <span className="text-sm">Account Loaded</span>
              </>
            ) : (
              <>
                <Wallet className="w-4 h-4 text-warning animate-pulse" />
                <span className="text-sm">Loading Account...</span>
              </>
            )}
          </div>
        )}
      </div>

      {error && (
        <div className="alert alert-error">
          <AlertCircle className="w-4 h-4" />
          <span>{error}</span>
        </div>
      )}

      {userAccount && (
        <div className="card bg-base-200 p-4">
          <h3 className="text-sm font-semibold mb-2">Account Info</h3>
          <div className="text-xs space-y-1">
            <div>
              <span className="opacity-70">Wallet: </span>
              <span className="font-mono">{publicKey?.toBase58().slice(0, 8)}...</span>
            </div>
            <div>
              <span className="opacity-70">Total Trades: </span>
              <span>{userAccount.total_trades || 0}</span>
            </div>
            <div>
              <span className="opacity-70">Total P/L: </span>
              <span className={userAccount.total_profit_loss >= 0 ? 'text-success' : 'text-error'}>
                {userAccount.total_profit_loss?.toFixed(4) || '0.0000'} SOL
              </span>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
