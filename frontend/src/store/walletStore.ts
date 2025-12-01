import { create } from 'zustand'
import { Keypair } from '@solana/web3.js'
import bs58 from 'bs58'

interface AssociatedWallet {
  publicKey: string
  privateKey: string // Encrypted in localStorage
  createdAt: string
  verified: boolean
}

interface WalletStore {
  associatedWallet: AssociatedWallet | null
  loading: boolean
  error: string | null
  
  // Load associated wallet from localStorage
  loadAssociatedWallet: (userPublicKey: string) => void
  
  // Create new associated wallet for user
  createAssociatedWallet: (userPublicKey: string, signature: Uint8Array) => Promise<void>
  
  // Clear associated wallet
  clearAssociatedWallet: () => void
  
  // Get keypair for trading
  getAssociatedKeypair: () => Keypair | null
}

// Simple encryption using signature as key (for demo - in production use better encryption)
function encryptPrivateKey(privateKey: string, signature: Uint8Array): string {
  // For now, we'll use base64 encoding with signature hash mixed in
  // In production, use proper encryption like AES
  const signatureB58 = bs58.encode(signature)
  return btoa(privateKey + ':::' + signatureB58.slice(0, 16))
}

// Commented out for future use with proper signature-based decryption
// function decryptPrivateKey(encrypted: string, signature: Uint8Array): string {
//   const signatureB58 = bs58.encode(signature)
//   const decrypted = atob(encrypted)
//   const [privateKey, sigPart] = decrypted.split(':::')
//   
//   if (sigPart !== signatureB58.slice(0, 16)) {
//     throw new Error('Invalid signature for decryption')
//   }
//   
//   return privateKey
// }

export const useWalletStore = create<WalletStore>((set, get) => ({
  associatedWallet: null,
  loading: false,
  error: null,
  
  loadAssociatedWallet: (userPublicKey: string) => {
    try {
      const stored = localStorage.getItem(`sol_beast_wallet_${userPublicKey}`)
      if (stored) {
        const wallet = JSON.parse(stored)
        set({ associatedWallet: wallet, error: null })
      } else {
        set({ associatedWallet: null })
      }
    } catch (err) {
      set({ 
        error: err instanceof Error ? err.message : 'Failed to load wallet',
        associatedWallet: null
      })
    }
  },
  
  createAssociatedWallet: async (userPublicKey: string, signature: Uint8Array) => {
    set({ loading: true, error: null })
    try {
      // Generate keypair from signature deterministically
      const seed = signature.slice(0, 32)
      const keypair = Keypair.fromSeed(seed)
      
      // Encrypt private key
      const privateKeyB58 = bs58.encode(keypair.secretKey)
      const encryptedPrivateKey = encryptPrivateKey(privateKeyB58, signature)
      
      const associatedWallet: AssociatedWallet = {
        publicKey: keypair.publicKey.toBase58(),
        privateKey: encryptedPrivateKey,
        createdAt: new Date().toISOString(),
        verified: true
      }
      
      // Save to localStorage
      localStorage.setItem(`sol_beast_wallet_${userPublicKey}`, JSON.stringify(associatedWallet))
      
      set({ associatedWallet, loading: false })
    } catch (err) {
      set({ 
        error: err instanceof Error ? err.message : 'Failed to create wallet',
        loading: false
      })
    }
  },
  
  clearAssociatedWallet: () => {
    set({ associatedWallet: null, error: null })
  },
  
  getAssociatedKeypair: (): Keypair | null => {
    const { associatedWallet } = get()
    if (!associatedWallet) return null
    
    try {
      // In production, we'd need the signature to decrypt
      // For now, return null - this would be called with proper signature
      return null
    } catch (err) {
      console.error('Failed to get keypair:', err)
      return null
    }
  }
}))
