import { useState, useEffect } from 'react'
import { Coins, ExternalLink, TrendingUp, Clock, User, CheckCircle, XCircle, ShoppingCart, Loader2 } from 'lucide-react'
import { botService } from '../services/botService'
import { useWallet } from '@solana/wallet-adapter-react'
import { WalletMultiButton } from '@solana/wallet-adapter-react-ui'
import { Transaction, TransactionInstruction, PublicKey, Connection } from '@solana/web3.js'
import { walletConnectRequiredToast, loadingToast, updateLoadingToast, transactionToastWithLink, errorToast } from '../utils/toast'

// DetectedToken interface matching the backend structure
interface DetectedToken {
  signature: string
  mint: string
  creator: string
  bonding_curve: string
  holder_address: string
  timestamp: string
  // Metadata (if available)
  name?: string
  symbol?: string
  image_uri?: string
  description?: string
  // Evaluation result
  should_buy: boolean
  evaluation_reason: string
  token_amount?: number
  buy_price_sol?: number
  // Additional info
  liquidity_sol?: number
}

export default function NewCoinsPanel() {
  const [tokens, setTokens] = useState<DetectedToken[]>([])
  const [filter, setFilter] = useState<'all' | 'pass' | 'fail'>('all')
  const [error, setError] = useState<string | null>(null)
  const [buyingToken, setBuyingToken] = useState<string | null>(null)
  
  const { publicKey, connected, sendTransaction } = useWallet()

  useEffect(() => {
    const fetchTokens = async () => {
      try {
        const data = await botService.getDetectedTokens()
        console.debug('Detected tokens response:', data)
        setTokens(data)
        setError(null)
      } catch (error) {
        console.error('Failed to fetch detected tokens:', error)
        setError(error instanceof Error ? error.message : String(error))
      }
    }
    
    // Fetch immediately
    fetchTokens()
    
    // Poll every 2 seconds
    const interval = setInterval(fetchTokens, 2000)
    return () => clearInterval(interval)
  }, [])

  const filteredTokens = filter === 'all' 
    ? tokens 
    : filter === 'pass'
    ? tokens.filter(t => t.should_buy)
    : tokens.filter(t => !t.should_buy)

  const passCount = tokens.filter(t => t.should_buy).length
  const failCount = tokens.filter(t => !t.should_buy).length
  
  const handleBuyToken = async (token: DetectedToken) => {
    if (!connected || !publicKey) {
      walletConnectRequiredToast()
      return
    }
    
    setBuyingToken(token.mint)
    const toastId = loadingToast('Building transaction...')
    
    try {
      console.log('Building buy transaction for token:', token.mint)
      
      // Step 1: Build transaction using WASM bot
      const txData = botService.buildBuyTransaction(token.mint, publicKey.toBase58())
      console.log('Transaction data:', txData)
      
      updateLoadingToast(toastId, true, 'Transaction built', 'Awaiting wallet signature...')
      
      // Step 2: Decode instruction data from base64
      const instructionData = Buffer.from(txData.data, 'base64')
      
      // Step 3: Convert accounts to web3.js format
      const keys = txData.accounts.map((acc: any) => ({
        pubkey: new PublicKey(acc.pubkey),
        isSigner: acc.isSigner,
        isWritable: acc.isWritable,
      }))
      
      // Step 4: Create transaction instruction
      const instruction = new TransactionInstruction({
        programId: new PublicKey(txData.programId),
        keys,
        data: instructionData,
      })
      
      // Step 5: Create transaction
      const transaction = new Transaction().add(instruction)
      
      // Step 6: Get RPC connection
      const settings = await botService.getSettings()
      const rpcUrl = settings.solana_rpc_urls?.[0] || 'https://api.mainnet-beta.solana.com'
      const connection = new Connection(rpcUrl, 'confirmed')
      
      // Step 7: Get recent blockhash
      const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash()
      transaction.recentBlockhash = blockhash
      transaction.lastValidBlockHeight = lastValidBlockHeight
      transaction.feePayer = publicKey
      
      console.log('Requesting wallet signature...')
      
      // Step 8: Sign and send transaction
      const signature = await sendTransaction(transaction, connection)
      
      console.log('Transaction sent:', signature)
      transactionToastWithLink(signature, 'buy', 'submitted')
      
      // Show loading toast for confirmation
      const confirmToastId = loadingToast('Confirming transaction...')
      
      // Step 9: Wait for confirmation
      const confirmation = await connection.confirmTransaction({
        signature,
        blockhash,
        lastValidBlockHeight
      }, 'confirmed')
      
      if (confirmation.value.err) {
        throw new Error('Transaction failed: ' + JSON.stringify(confirmation.value.err))
      }
      
      console.log('Transaction confirmed!')
      updateLoadingToast(confirmToastId, true, 'Transaction confirmed!', 'Purchase successful')
      transactionToastWithLink(signature, 'buy', 'confirmed')
      
    } catch (err) {
      console.error('Buy failed:', err)
      errorToast('Failed to buy token', err instanceof Error ? err.message : String(err))
    } finally {
      setBuyingToken(null)
    }
  }

  if (error) {
    return (
      <div className="card-enhanced rounded-xl p-12 text-center">
        <XCircle size={48} className="mx-auto text-red-500 mb-4" />
        <p className="text-red-400 mb-2">Failed to fetch detected tokens</p>
        <p className="text-gray-500 text-sm">{error}</p>
      </div>
    )
  }

  if (tokens.length === 0) {
    return (
      <div className="card-enhanced rounded-xl p-12 text-center">
        <Coins size={48} className="mx-auto text-gray-500 mb-4 opacity-50" />
        <p className="text-gray-400">No new tokens detected yet</p>
        <p className="text-gray-500 text-sm">Bot is monitoring for new token launches</p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Filter Tabs */}
      <div className="card-enhanced rounded-xl p-4">
        <div className="flex gap-2">
          <button
            onClick={() => setFilter('all')}
            className={`px-4 py-2 rounded-lg transition-colors ${
              filter === 'all'
                ? 'bg-gradient-to-r from-sol-purple to-sol-cyan text-white shadow-glow'
                : 'bg-sol-darker text-gray-400 hover:bg-gray-700'
            }`}
          >
            All ({tokens.length})
          </button>
          <button
            onClick={() => setFilter('pass')}
            className={`px-4 py-2 rounded-lg transition-colors ${
              filter === 'pass'
                ? 'bg-gradient-to-r from-green-600 to-green-400 text-white shadow-glow'
                : 'bg-sol-darker text-gray-400 hover:bg-gray-700'
            }`}
          >
            ✓ Pass ({passCount})
          </button>
          <button
            onClick={() => setFilter('fail')}
            className={`px-4 py-2 rounded-lg transition-colors ${
              filter === 'fail'
                ? 'bg-gradient-to-r from-red-600 to-red-400 text-white shadow-glow'
                : 'bg-sol-darker text-gray-400 hover:bg-gray-700'
            }`}
          >
            ✗ Fail ({failCount})
          </button>
        </div>
      </div>

      {/* Tokens Grid */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        {filteredTokens.map((token) => (
          <div 
            key={token.signature} 
            className={`card-enhanced rounded-xl p-6 transition-colors ${
              token.should_buy 
                ? 'hover:border-green-500 border-l-4 border-l-green-500' 
                : 'hover:border-red-500 border-l-4 border-l-red-500'
            }`}
          >
            <div className="flex gap-4">
              {/* Token Image */}
              <div className="flex-shrink-0">
                {token.image_uri ? (
                  <img 
                    src={token.image_uri} 
                    alt={token.name || token.symbol || 'Token'} 
                    className="w-16 h-16 rounded-lg object-cover"
                    onError={(e) => {
                      e.currentTarget.src = 'data:image/svg+xml,<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="%23374151"/><text x="32" y="32" font-size="24" text-anchor="middle" dy=".3em" fill="%239CA3AF">?</text></svg>'
                    }}
                  />
                ) : (
                  <div className="w-16 h-16 rounded-lg bg-gray-700 flex items-center justify-center">
                    <Coins size={32} className="text-gray-500" />
                  </div>
                )}
              </div>

              {/* Token Info */}
              <div className="flex-1 min-w-0">
                <div className="flex items-start justify-between mb-2">
                  <div>
                    <h3 className="text-lg font-semibold truncate">
                      {token.name || token.symbol || 'Unknown Token'}
                    </h3>
                    {token.symbol && token.name && (
                      <p className="text-sm text-gray-400">${token.symbol}</p>
                    )}
                  </div>
                  <div title={token.should_buy ? "Passed evaluation" : "Failed evaluation"}>
                    {token.should_buy ? (
                      <CheckCircle size={24} className="text-green-400 flex-shrink-0" />
                    ) : (
                      <XCircle size={24} className="text-red-400 flex-shrink-0" />
                    )}
                  </div>
                </div>

                {/* Evaluation Result */}
                <div className={`mb-3 p-2 rounded text-xs ${
                  token.should_buy 
                    ? 'bg-green-900/20 text-green-300 border border-green-700' 
                    : 'bg-red-900/20 text-red-300 border border-red-700'
                }`}>
                  {token.evaluation_reason}
                </div>

                {/* Description */}
                {token.description && (
                  <p className="text-xs text-gray-400 mb-3 overflow-hidden" style={{ 
                    display: '-webkit-box',
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: 'vertical',
                    lineClamp: 2
                  }}>
                    {token.description}
                  </p>
                )}

                {/* Details */}
                <div className="space-y-2 text-sm">
                  <div className="flex items-center gap-2 text-gray-400">
                    <Clock size={14} />
                    <span className="text-xs">
                      {new Date(token.timestamp).toLocaleString()}
                    </span>
                  </div>

                  <div className="flex items-center gap-2">
                    <User size={14} className="text-gray-400" />
                    <span className="text-xs font-mono text-gray-400 truncate">
                      Creator: {token.creator.slice(0, 8)}...{token.creator.slice(-8)}
                    </span>
                    <a
                      href={`https://solscan.io/account/${token.creator}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sol-purple hover:text-sol-purple-light transition-colors"
                    >
                      <ExternalLink size={14} />
                    </a>
                  </div>

                  <div className="flex items-center gap-2">
                    <TrendingUp size={14} className="text-gray-400" />
                    <span className="text-xs font-mono text-gray-400 truncate">
                      Curve: {token.bonding_curve.slice(0, 8)}...{token.bonding_curve.slice(-8)}
                    </span>
                    <a
                      href={`https://solscan.io/account/${token.bonding_curve}`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="text-sol-purple hover:text-sol-purple-light transition-colors"
                    >
                      <ExternalLink size={14} />
                    </a>
                  </div>

                  {/* Price and Liquidity Info */}
                  {(token.buy_price_sol || token.liquidity_sol) && (
                    <div className="pt-2 border-t border-gray-700 space-y-1">
                      {token.buy_price_sol && (
                        <div className="flex items-center justify-between">
                          <span className="text-xs text-gray-400">Buy Price:</span>
                          <span className="text-xs font-semibold text-sol-purple">
                            {token.buy_price_sol.toFixed(9)} SOL
                          </span>
                        </div>
                      )}
                      {token.liquidity_sol && (
                        <div className="flex items-center justify-between">
                          <span className="text-xs text-gray-400">Liquidity:</span>
                          <span className="text-xs font-semibold text-sol-cyan">
                            {token.liquidity_sol.toFixed(4)} SOL
                          </span>
                        </div>
                      )}
                      {token.token_amount && (
                        <div className="flex items-center justify-between">
                          <span className="text-xs text-gray-400">Token Amount:</span>
                          <span className="text-xs font-mono text-gray-300">
                            {token.token_amount.toLocaleString()}
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </div>

                {/* Mint Address */}
                <div className="mt-3 pt-3 border-t border-gray-700">
                  <div className="flex items-center justify-between gap-2">
                    <span className="text-xs font-mono text-gray-500 truncate">
                      {token.mint}
                    </span>
                    <div className="flex gap-2">
                      <a
                        href={`https://solscan.io/token/${token.mint}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-sol-purple hover:text-sol-purple-light transition-colors"
                        title="View on Solscan"
                      >
                        <ExternalLink size={16} />
                      </a>
                      <button
                        onClick={() => navigator.clipboard.writeText(token.mint)}
                        className="text-gray-400 hover:text-white transition-colors"
                        title="Copy mint address"
                      >
                        <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                          <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                        </svg>
                      </button>
                    </div>
                  </div>
                </div>
                
                {/* Buy Button (only for tokens that passed evaluation) */}
                {token.should_buy && (
                  <div className="mt-3 pt-3 border-t border-gray-700">
                    {connected ? (
                      <button
                        onClick={() => handleBuyToken(token)}
                        disabled={buyingToken === token.mint}
                        className={`w-full py-2 px-4 rounded-lg font-semibold transition-all flex items-center justify-center gap-2 ${
                          buyingToken === token.mint
                            ? 'bg-gray-600 text-gray-400 cursor-not-allowed'
                            : 'bg-gradient-to-r from-green-600 to-green-400 hover:from-green-700 hover:to-green-500 text-white shadow-glow'
                        }`}
                      >
                        {buyingToken === token.mint ? (
                          <>
                            <Loader2 size={16} className="animate-spin" />
                            <span>Processing...</span>
                          </>
                        ) : (
                          <>
                            <ShoppingCart size={16} />
                            <span>Buy Token</span>
                          </>
                        )}
                      </button>
                    ) : (
                      <WalletMultiButton className="!w-full !bg-gradient-to-r !from-purple-600 !to-blue-600 hover:!from-purple-700 hover:!to-blue-700" />
                    )}
                  </div>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  )
}
