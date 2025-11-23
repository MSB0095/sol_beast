/**
 * Client-side trading engine that runs in the browser
 * This replaces the backend Rust service with browser-based WebSocket monitoring
 */

import { Connection } from '@solana/web3.js'
import { WalletContextState } from '@solana/wallet-adapter-react'

// Configuration constants
const PUMP_FUN_PROGRAM_ID = '6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P'

export interface TradingEngineConfig {
  rpcUrl: string
  wsUrl: string
  buyAmount: number
  tpPercent: number
  slPercent: number
  maxHeldCoins: number
  enableSaferSniping: boolean
  minTokensThreshold: number
  maxSolPerToken: number
}

export interface TokenDetection {
  mint: string
  creator: string
  signature: string
  timestamp: number
}

export interface Holding {
  mint: string
  amount: number
  buyPrice: number
  buyTime: number
  name?: string
  symbol?: string
  image?: string
}

export class TradingEngine {
  private connection: Connection
  private wallet: WalletContextState | null = null
  private config: TradingEngineConfig
  private holdings: Map<string, Holding> = new Map()
  private isRunning: boolean = false
  private websocket: WebSocket | null = null
  private monitorInterval: NodeJS.Timeout | null = null

  constructor(config: TradingEngineConfig) {
    this.config = config
    this.connection = new Connection(config.rpcUrl, 'confirmed')
  }

  setWallet(wallet: WalletContextState) {
    this.wallet = wallet
  }

  async start() {
    if (this.isRunning) {
      console.log('Trading engine already running')
      return
    }

    if (!this.wallet || !this.wallet.connected) {
      throw new Error('Wallet must be connected to start trading')
    }

    this.isRunning = true
    console.log('Starting trading engine in browser...')

    // Start monitoring for new tokens
    await this.startTokenMonitoring()

    // Start monitoring existing holdings
    this.startHoldingsMonitor()
  }

  async stop() {
    this.isRunning = false
    
    if (this.websocket) {
      this.websocket.close()
      this.websocket = null
    }

    if (this.monitorInterval) {
      clearInterval(this.monitorInterval)
      this.monitorInterval = null
    }

    console.log('Trading engine stopped')
  }

  private async startTokenMonitoring() {
    // Connect to Solana WebSocket to monitor for new pump.fun tokens
    const wsUrl = this.config.wsUrl
    
    this.websocket = new WebSocket(wsUrl)
    
    this.websocket.onopen = () => {
      console.log('Connected to Solana WebSocket')
      
      // Subscribe to pump.fun program logs
      const subscribeMessage = {
        jsonrpc: '2.0',
        id: 1,
        method: 'logsSubscribe',
        params: [
          {
            mentions: [PUMP_FUN_PROGRAM_ID]
          },
          {
            commitment: 'confirmed'
          }
        ]
      }
      
      this.websocket?.send(JSON.stringify(subscribeMessage))
    }

    this.websocket.onmessage = async (event) => {
      try {
        const data = JSON.parse(event.data)
        
        if (data.method === 'logsNotification') {
          await this.handleLogNotification(data.params)
        }
      } catch (error) {
        console.error('Error processing WebSocket message:', error)
      }
    }

    this.websocket.onerror = (error) => {
      console.error('WebSocket error:', error)
    }

    this.websocket.onclose = () => {
      console.log('WebSocket connection closed')
      
      // Attempt to reconnect if still running
      if (this.isRunning) {
        console.log('Attempting to reconnect in 5 seconds...')
        setTimeout(() => {
          if (this.isRunning) {
            this.startTokenMonitoring()
          }
        }, 5000)
      }
    }
  }

  private async handleLogNotification(params: any) {
    const logs = params?.result?.value?.logs || []
    const signature = params?.result?.value?.signature

    // Check if this is a new token creation (InitializeMint2)
    const isNewToken = logs.some((log: string) => 
      log.includes('Instruction: InitializeMint2')
    )

    if (isNewToken && signature) {
      console.log('New token detected:', signature)
      
      // Check if we're at max holdings
      if (this.holdings.size >= this.config.maxHeldCoins) {
        console.log('Max holdings reached, skipping buy')
        return
      }

      try {
        await this.attemptBuy(signature)
      } catch (error) {
        console.error('Error attempting buy:', error)
      }
    }
  }

  private async attemptBuy(signature: string) {
    // Fetch transaction details to get mint address
    const tx = await this.connection.getTransaction(signature, {
      commitment: 'confirmed',
      maxSupportedTransactionVersion: 0
    })

    if (!tx) {
      console.log('Transaction not found:', signature)
      return
    }

    // Extract mint address from transaction
    // This is a simplified version - in production you'd need more robust parsing
    const accountKeys = tx.transaction.message.getAccountKeys()
    
    // For pump.fun, the mint is typically one of the account keys
    // You'd need to implement proper transaction parsing here
    console.log('Transaction accounts:', accountKeys)

    // TODO: Implement actual buy logic:
    // 1. Parse transaction to extract mint address
    // 2. Fetch token metadata
    // 3. Calculate price and check safety filters
    // 4. Build and send buy transaction using wallet
    // 5. Add to holdings
  }

  private startHoldingsMonitor() {
    // Monitor holdings every 5 seconds for TP/SL
    this.monitorInterval = setInterval(async () => {
      for (const [mint, holding] of this.holdings) {
        try {
          await this.checkHoldingForExit(mint, holding)
        } catch (error) {
          console.error(`Error monitoring holding ${mint}:`, error)
        }
      }
    }, 5000) as any
  }

  private async checkHoldingForExit(mint: string, holding: Holding) {
    // TODO: Implement TP/SL checking:
    // 1. Fetch current price
    // 2. Calculate profit/loss percentage
    // 3. Check if TP or SL is hit
    // 4. If so, execute sell transaction
    
    const currentPrice = await this.getCurrentPrice(mint)
    if (!currentPrice) return

    const profitPercent = ((currentPrice - holding.buyPrice) / holding.buyPrice) * 100

    if (profitPercent >= this.config.tpPercent) {
      console.log(`Take profit hit for ${mint}: ${profitPercent.toFixed(2)}%`)
      await this.sell(mint, 'TP')
    } else if (profitPercent <= this.config.slPercent) {
      console.log(`Stop loss hit for ${mint}: ${profitPercent.toFixed(2)}%`)
      await this.sell(mint, 'SL')
    }
  }

  private async getCurrentPrice(_mint: string): Promise<number | null> {
    // TODO: Implement price fetching from bonding curve
    // This would involve fetching the bonding curve account and calculating price
    return null
  }

  private async sell(mint: string, reason: string) {
    // TODO: Implement sell transaction
    console.log(`Selling ${mint} - reason: ${reason}`)
    
    // Remove from holdings after sell
    this.holdings.delete(mint)
  }

  getHoldings(): Holding[] {
    return Array.from(this.holdings.values())
  }

  isEngineRunning(): boolean {
    return this.isRunning
  }
}

// Singleton instance
let engineInstance: TradingEngine | null = null

export function getTradingEngine(config?: TradingEngineConfig): TradingEngine {
  if (!engineInstance && config) {
    engineInstance = new TradingEngine(config)
  }
  
  if (!engineInstance) {
    throw new Error('Trading engine not initialized')
  }
  
  return engineInstance
}
