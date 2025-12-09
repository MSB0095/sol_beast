import { useState, useEffect } from 'react'
import { Clock, AlertTriangle } from 'lucide-react'
import { botService } from '../services/botService'
import { useWallet } from '@solana/wallet-adapter-react'
import { Transaction, TransactionInstruction, PublicKey, Connection } from '@solana/web3.js'
import { walletConnectRequiredToast, loadingToast, updateLoadingToast, transactionToastWithLink, errorToast } from '../utils/toast'

interface Holding {
  mint: string
  amount: number
  buy_price: number
  buy_time: string
  metadata?: {
    name?: string
    symbol?: string
    image?: string
    description?: string
  }
  onchain?: {
    name?: string
    symbol?: string
  }
}

interface MonitorAction {
  action: string
  mint: string
  reason: string
  profitPercent: number
  currentPrice: number
  amount: number
}

export default function HoldingsPanel() {
  const [holdings, setHoldings] = useState<Holding[]>([])
  const [alerts, setAlerts] = useState<MonitorAction[]>([])
  const [error, setError] = useState<string | null>(null)
  const [sellingToken, setSellingToken] = useState<string | null>(null)
  
  const { publicKey, connected, sendTransaction } = useWallet()

  // Fetch holdings
  useEffect(() => {
    const fetchHoldings = async () => {
      try {
        const data = await botService.getHoldings()
        console.debug('Holdings response:', data)
        setHoldings(data)
        setError(null)
      } catch (error) {
        console.error('Failed to fetch holdings:', error)
        setError(error instanceof Error ? error.message : String(error))
      }
    }
    
    fetchHoldings()
    const interval = setInterval(fetchHoldings, 5000) // Refresh every 5 seconds
    return () => clearInterval(interval)
  }, [])

  // Monitor holdings for TP/SL/timeout
  useEffect(() => {
    const monitor = async () => {
      try {
        const result = await botService.monitorHoldings()
        console.debug('Monitor result:', result)
        
        if (result.action === 'sell_required' && result.actions) {
          setAlerts(result.actions)
        } else {
          setAlerts([])
        }
      } catch (error) {
        console.error('Failed to monitor holdings:', error)
      }
    }
    
    // Only monitor if we have holdings and bot is running
    if (holdings.length > 0) {
      monitor()
      const interval = setInterval(monitor, 10000) // Check every 10 seconds
      return () => clearInterval(interval)
    }
  }, [holdings.length])

  const handleSellToken = async (holding: Holding, alert?: MonitorAction) => {
    if (!connected || !publicKey) {
      walletConnectRequiredToast()
      return
    }
    
    setSellingToken(holding.mint)
    const toastId = loadingToast('Building sell transaction...')
    let confirmToastId: string | undefined
    
    try {
      console.log('Building sell transaction for token:', holding.mint)
      
      // Step 1: Build transaction using WASM bot
      const txData = botService.buildSellTransaction(holding.mint, publicKey.toBase58())
      console.log('Sell transaction data:', txData)
      
      updateLoadingToast(toastId, true, 'Transaction built', 'Awaiting wallet signature...')
      
      // Step 2: Decode instruction data from base64
      const instructionData = Buffer.from(txData.data, 'base64')
      
      // Step 3: Convert accounts to web3.js format
      const keys = txData.accounts.map((acc: { pubkey: string; isSigner: boolean; isWritable: boolean }) => ({
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
      
      console.log('Sell transaction sent:', signature)
      transactionToastWithLink(signature, 'sell', 'submitted')
      
      // Show loading toast for confirmation
      confirmToastId = loadingToast('Confirming transaction...')
      
      // Step 9: Wait for confirmation with timeout handling
      const confirmation = await Promise.race([
        connection.confirmTransaction({
          signature,
          blockhash,
          lastValidBlockHeight
        }, 'confirmed'),
        new Promise<never>((_, reject) => 
          setTimeout(() => reject(new Error('Confirmation timeout after 60s')), 60000)
        )
      ])
      
      if (confirmation.value.err) {
        throw new Error('Transaction failed: ' + JSON.stringify(confirmation.value.err))
      }
      
      // Only proceed if confirmation succeeded
      if (!confirmation.value.err) {
        console.log('Sell transaction confirmed!')
        updateLoadingToast(confirmToastId, true, 'Transaction confirmed!', 'Sale successful')
        transactionToastWithLink(signature, 'sell', 'confirmed')
        
        // Step 10: Remove the holding after successful confirmation
        try {
          const reason = alert?.reason || 'MANUAL'
          const profitPercent = alert?.profitPercent || 0
          botService.removeHolding(holding.mint, profitPercent, reason)
          console.log('Holding removed successfully')
          
          // Refresh holdings immediately
          const data = await botService.getHoldings()
          setHoldings(data)
        } catch (holdingErr) {
          console.error('Failed to remove holding:', holdingErr)
          errorToast('Failed to remove holding', holdingErr instanceof Error ? holdingErr.message : String(holdingErr))
        }
      }
      
    } catch (err) {
      console.error('Sell failed:', err)
      
      // Determine which toast to update based on where the error occurred
      const errorMessage = err instanceof Error ? err.message : String(err)
      if (confirmToastId) {
        // Error occurred during confirmation
        updateLoadingToast(confirmToastId, false, 'Transaction failed', errorMessage)
      } else {
        // Error occurred before transaction was sent
        updateLoadingToast(toastId, false, 'Transaction failed', errorMessage)
      }
    } finally {
      setSellingToken(null)
    }
  }

  if (error) {
    return (
      <div className="card-enhanced rounded-xl p-12 text-center">
        <AlertTriangle size={48} className="mx-auto text-red-500 mb-4" />
        <p className="text-red-400 mb-2">Failed to fetch holdings</p>
        <p className="text-gray-500 text-sm">{error}</p>
      </div>
    )
  }

  if (holdings.length === 0) {
    return (
      <div className="card-enhanced rounded-xl p-12 text-center">
        <Clock size={48} className="mx-auto text-gray-500 mb-4 opacity-50" />
        <p className="text-gray-400 font-semibold">No active holdings</p>
        <p className="text-gray-500 text-sm mt-2">Trades will appear here when they're active</p>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* Alerts for TP/SL/Timeout */}
      {alerts.length > 0 && (
        <div className="card-enhanced rounded-xl p-4 border-2 border-yellow-500/50 bg-yellow-500/5">
          <h3 className="text-lg font-semibold mb-2 text-yellow-400 flex items-center gap-2">
            <AlertTriangle size={20} />
            Sell Alerts ({alerts.length})
          </h3>
          <div className="space-y-2">
            {alerts.map((alert) => {
              const holding = holdings.find(h => h.mint === alert.mint)
              const symbol = holding?.metadata?.symbol || holding?.mint.slice(0, 8)
              return (
                <div key={alert.mint} className="flex items-center justify-between p-2 bg-gray-800 rounded">
                  <div>
                    <span className="font-semibold">{symbol}</span>
                    <span className="text-sm text-gray-400 ml-2">
                      {alert.reason}: {alert.profitPercent > 0 ? '+' : ''}{alert.profitPercent.toFixed(2)}%
                    </span>
                  </div>
                  <button
                    onClick={() => holding && handleSellToken(holding, alert)}
                    disabled={sellingToken === alert.mint || !connected}
                    className="px-3 py-1 bg-red-600 hover:bg-red-700 disabled:bg-gray-600 rounded text-sm font-semibold transition-colors"
                  >
                    Sell Now
                  </button>
                </div>
              )
            })}
          </div>
        </div>
      )}

      {/* Holdings Table */}
      <div className="card-enhanced rounded-xl p-6">
        <h3 className="text-lg font-semibold mb-4 gradient-text">Current Holdings ({holdings.length})</h3>
        
        <div className="overflow-x-auto">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-gray-400 border-b border-gray-700">
                <th className="text-left py-3 px-4">Token</th>
                <th className="text-left py-3 px-4">Name/Symbol</th>
                <th className="text-right py-3 px-4">Buy Price</th>
                <th className="text-right py-3 px-4">Tokens</th>
                <th className="text-right py-3 px-4">Hold Time</th>
                <th className="text-center py-3 px-4">Actions</th>
              </tr>
            </thead>
            <tbody>
              {holdings.map((holding) => {
                const mint = holding.mint
                const symbol = holding.metadata?.symbol || holding.onchain?.symbol
                const name = holding.metadata?.name || holding.onchain?.name
                const image = holding.metadata?.image
                const holdTime = Math.floor((Date.now() - new Date(holding.buy_time).getTime()) / 1000)
                const minutes = Math.floor(holdTime / 60)
                const seconds = holdTime % 60
                const alert = alerts.find(a => a.mint === mint)

                return (
                  <tr key={mint} className={`border-b border-gray-700/50 hover:bg-sol-darker/50 transition-colors ${alert ? 'bg-yellow-500/5' : ''}`}>
                    <td className="py-3 px-4">
                      <div className="flex items-center gap-2">
                        {image ? (
                          <img 
                            src={image} 
                            alt={symbol || name || 'Token'} 
                            className="w-8 h-8 rounded"
                            onError={(e) => {
                              e.currentTarget.style.display = 'none'
                            }}
                          />
                        ) : (
                          <div className="w-8 h-8 rounded bg-gray-700 flex items-center justify-center text-gray-500 text-xs">
                            ?
                          </div>
                        )}
                        <span className="font-mono text-xs text-gray-400">
                          {mint.slice(0, 6)}...{mint.slice(-4)}
                        </span>
                      </div>
                    </td>
                    <td className="py-3 px-4">
                      {name || symbol ? (
                        <div>
                          <div className="font-semibold">{name || symbol}</div>
                          {symbol && name && (
                            <div className="text-xs text-gray-500">${symbol}</div>
                          )}
                        </div>
                      ) : (
                        <span className="text-gray-500">Unknown</span>
                      )}
                    </td>
                    <td className="py-3 px-4 text-right font-mono text-xs">
                      {holding.buy_price.toFixed(9)} SOL
                    </td>
                    <td className="py-3 px-4 text-right font-mono">
                      {(holding.amount / 1_000_000).toLocaleString()}
                    </td>
                    <td className="py-3 px-4 text-right text-gray-400 text-xs">
                      {minutes}m {seconds}s
                    </td>
                    <td className="py-3 px-4 text-center">
                      <div className="flex items-center justify-center gap-2">
                        <a
                          href={`https://solscan.io/token/${mint}`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-sol-purple hover:text-sol-purple-light transition-colors"
                          title="View on Solscan"
                        >
                          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                            <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
                            <polyline points="15 3 21 3 21 9"></polyline>
                            <line x1="10" y1="14" x2="21" y2="3"></line>
                          </svg>
                        </a>
                        <button
                          onClick={() => handleSellToken(holding, alert)}
                          disabled={sellingToken === mint || !connected}
                          className="px-2 py-1 text-xs bg-red-600 hover:bg-red-700 disabled:bg-gray-600 rounded font-semibold transition-colors"
                        >
                          {sellingToken === mint ? 'Selling...' : 'Sell'}
                        </button>
                      </div>
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </div>
      </div>

      {/* Summary */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm font-medium mb-2">Total Holdings</p>
          <p className="text-2xl font-bold gradient-text">
            {holdings.length} position{holdings.length !== 1 ? 's' : ''}
          </p>
        </div>

        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm font-medium mb-2">Sell Alerts</p>
          <p className="text-2xl font-bold text-yellow-400">
            {alerts.length}
          </p>
        </div>

        <div className="card-enhanced rounded-xl p-4">
          <p className="text-gray-400 text-sm font-medium mb-2">Total Tokens</p>
          <p className="text-2xl font-bold gradient-text">
            {holdings.reduce((sum, h) => sum + h.amount, 0) / 1_000_000} M
          </p>
        </div>
      </div>
    </div>
  )
}
