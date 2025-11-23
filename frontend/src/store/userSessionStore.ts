import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import { PublicKey } from '@solana/web3.js'

export interface UserSettings {
  // Trading settings per user
  buyAmount: number
  tpPercent: number
  slPercent: number
  maxHeldCoins: number
  enableSaferSniping: boolean
  minTokensThreshold: number
  maxSolPerToken: number
  slippageBps: number
  minLiquiditySol: number
  maxLiquiditySol: number
  
  // User preferences
  theme?: string
  notifications?: boolean
  
  // Timestamps
  createdAt: string
  lastActive: string
}

interface UserSessionState {
  // Current connected wallet
  walletPublicKey: string | null
  
  // User sessions (keyed by wallet public key)
  userSessions: Record<string, UserSettings>
  
  // Actions
  setWallet: (publicKey: PublicKey | null) => void
  getCurrentUserSettings: () => UserSettings | null
  updateUserSettings: (settings: Partial<UserSettings>) => void
  clearSession: () => void
  getAllSessions: () => Record<string, UserSettings>
}

const DEFAULT_USER_SETTINGS: Omit<UserSettings, 'createdAt' | 'lastActive'> = {
  buyAmount: 0.1,
  tpPercent: 30.0,
  slPercent: -20.0,
  maxHeldCoins: 10,
  enableSaferSniping: true,
  minTokensThreshold: 1000000,
  maxSolPerToken: 0.0001,
  slippageBps: 500,
  minLiquiditySol: 0.0,
  maxLiquiditySol: 100.0,
  theme: 'sol-green',
  notifications: true,
}

export const useUserSessionStore = create<UserSessionState>()(
  persist(
    (set, get) => ({
      walletPublicKey: null,
      userSessions: {},

      setWallet: (publicKey: PublicKey | null) => {
        const pubKeyString = publicKey?.toString() || null
        
        set({ walletPublicKey: pubKeyString })
        
        // Initialize or update user session
        if (pubKeyString) {
          const sessions = get().userSessions
          const now = new Date().toISOString()
          
          if (!sessions[pubKeyString]) {
            // Create new user session
            sessions[pubKeyString] = {
              ...DEFAULT_USER_SETTINGS,
              createdAt: now,
              lastActive: now,
            }
          } else {
            // Update last active time
            sessions[pubKeyString].lastActive = now
          }
          
          set({ userSessions: { ...sessions } })
        }
      },

      getCurrentUserSettings: () => {
        const { walletPublicKey, userSessions } = get()
        if (!walletPublicKey) return null
        return userSessions[walletPublicKey] || null
      },

      updateUserSettings: (settings: Partial<UserSettings>) => {
        const { walletPublicKey, userSessions } = get()
        if (!walletPublicKey) return
        
        const currentSettings = userSessions[walletPublicKey]
        if (!currentSettings) return
        
        userSessions[walletPublicKey] = {
          ...currentSettings,
          ...settings,
          lastActive: new Date().toISOString(),
        }
        
        set({ userSessions: { ...userSessions } })
      },

      clearSession: () => {
        set({ walletPublicKey: null })
      },

      getAllSessions: () => {
        return get().userSessions
      },
    }),
    {
      name: 'sol-beast-user-sessions',
      partialize: (state) => ({
        // Only persist user sessions, not current wallet
        userSessions: state.userSessions,
      }),
    }
  )
)
