import { useState, useEffect } from 'react'
import { botService } from '../services/botService'

interface RPCConfig {
  httpsUrls: string[]
  wssUrls: string[]
}

interface RPCConfigModalProps {
  onConfigured: () => void
}

const STORAGE_KEY = 'sol_beast_user_rpc_config'

// Check if RPC config exists and is valid
export function hasValidRPCConfig(): boolean {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (!stored) return false
    
    const config: RPCConfig = JSON.parse(stored)
    return (
      Array.isArray(config.httpsUrls) &&
      config.httpsUrls.length > 0 &&
      config.httpsUrls.every(url => url.startsWith('https://')) &&
      Array.isArray(config.wssUrls) &&
      config.wssUrls.length > 0 &&
      config.wssUrls.every(url => url.startsWith('wss://'))
    )
  } catch {
    return false
  }
}

// Get stored RPC config
export function getStoredRPCConfig(): RPCConfig | null {
  try {
    const stored = localStorage.getItem(STORAGE_KEY)
    if (!stored) return null
    return JSON.parse(stored)
  } catch {
    return null
  }
}

export default function RPCConfigModal({ onConfigured }: RPCConfigModalProps) {
  const [httpsUrl, setHttpsUrl] = useState('')
  const [wssUrl, setWssUrl] = useState('')
  const [httpsUrls, setHttpsUrls] = useState<string[]>([])
  const [wssUrls, setWssUrls] = useState<string[]>([])
  const [error, setError] = useState<string | null>(null)
  const [testing, setTesting] = useState(false)
  const [testResults, setTestResults] = useState<{ https?: string, wss?: string }>({})

  // Load existing config if any
  useEffect(() => {
    const config = getStoredRPCConfig()
    if (config) {
      setHttpsUrls(config.httpsUrls)
      setWssUrls(config.wssUrls)
    }
  }, [])

  const validateHttpsUrl = (url: string): boolean => {
    return url.trim().startsWith('https://')
  }

  const validateWssUrl = (url: string): boolean => {
    return url.trim().startsWith('wss://')
  }

  const addHttpsUrl = () => {
    const trimmed = httpsUrl.trim()
    if (!trimmed) {
      setError('Please enter an HTTPS RPC URL')
      return
    }
    if (!validateHttpsUrl(trimmed)) {
      setError('HTTPS URL must start with https://')
      return
    }
    if (httpsUrls.includes(trimmed)) {
      setError('This URL is already added')
      return
    }
    setHttpsUrls([...httpsUrls, trimmed])
    setHttpsUrl('')
    setError(null)
  }

  const addWssUrl = () => {
    const trimmed = wssUrl.trim()
    if (!trimmed) {
      setError('Please enter a WSS WebSocket URL')
      return
    }
    if (!validateWssUrl(trimmed)) {
      setError('WSS URL must start with wss://')
      return
    }
    if (wssUrls.includes(trimmed)) {
      setError('This URL is already added')
      return
    }
    setWssUrls([...wssUrls, trimmed])
    setWssUrl('')
    setError(null)
  }

  const removeHttpsUrl = (index: number) => {
    setHttpsUrls(httpsUrls.filter((_, i) => i !== index))
  }

  const removeWssUrl = (index: number) => {
    setWssUrls(wssUrls.filter((_, i) => i !== index))
  }

  const testConnections = async () => {
    if (httpsUrls.length === 0 || wssUrls.length === 0) {
      setError('Please add at least one HTTPS and one WSS URL')
      return
    }

    setTesting(true)
    setTestResults({})
    setError(null)

    try {
      // Test HTTPS connection
      try {
        const httpsTest = await fetch(httpsUrls[0], {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            jsonrpc: '2.0',
            id: 1,
            method: 'getHealth',
          }),
        })

        if (httpsTest.ok) {
          setTestResults(prev => ({ ...prev, https: 'Connected ✓' }))
        } else {
          setTestResults(prev => ({ ...prev, https: `Failed: ${httpsTest.statusText}` }))
        }
      } catch (httpsErr) {
        const errorMsg = httpsErr instanceof Error ? httpsErr.message : 'Unknown error'
        // Check for CORS errors
        if (errorMsg.includes('CORS') || errorMsg.includes('cors') || errorMsg.includes('Failed to fetch')) {
          setTestResults(prev => ({ ...prev, https: 'CORS Error - Endpoint may not support browser requests' }))
        } else {
          setTestResults(prev => ({ ...prev, https: `Failed: ${errorMsg}` }))
        }
      }

      // Test WSS connection
      const ws = new WebSocket(wssUrls[0])
      let timeoutId: number | null = null
      let connectionClosed = false
      
      await new Promise<void>((resolve, reject) => {
        timeoutId = setTimeout(() => {
          if (!connectionClosed) {
            connectionClosed = true
            ws.close()
            reject(new Error('Connection timeout'))
          }
        }, 5000)

        ws.onopen = () => {
          if (timeoutId) clearTimeout(timeoutId)
          if (!connectionClosed) {
            connectionClosed = true
            setTestResults(prev => ({ ...prev, wss: 'Connected ✓' }))
            ws.close()
            resolve()
          }
        }

        ws.onerror = () => {
          if (timeoutId) clearTimeout(timeoutId)
          if (!connectionClosed) {
            connectionClosed = true
            ws.close()
            reject(new Error('WebSocket connection failed'))
          }
        }

        ws.onclose = () => {
          if (timeoutId) clearTimeout(timeoutId)
          connectionClosed = true
        }
      })
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Connection test failed')
      setTestResults(prev => {
        if (!prev.wss) {
          return { ...prev, wss: 'Failed ✗' }
        }
        return prev
      })
    } finally {
      setTesting(false)
    }
  }

  const saveAndContinue = async () => {
    if (httpsUrls.length === 0) {
      setError('Please add at least one HTTPS RPC URL')
      return
    }
    if (wssUrls.length === 0) {
      setError('Please add at least one WSS WebSocket URL')
      return
    }

    const config: RPCConfig = {
      httpsUrls,
      wssUrls,
    }

    // Save to localStorage
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config))

    // Update bot service settings
    try {
      await botService.init() // Initialize WASM first if needed
      const settings = await botService.getSettings()
      settings.solana_rpc_urls = httpsUrls
      settings.solana_ws_urls = wssUrls
      await botService.updateSettings(settings)
    } catch (err) {
      console.warn('Could not update bot settings immediately:', err)
    }

    onConfigured()
  }

  const canContinue = httpsUrls.length > 0 && wssUrls.length > 0

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/95 backdrop-blur-sm">
      <div className="w-full max-w-2xl mx-4 cyber-card p-8 max-h-[90vh] overflow-y-auto">
        <div className="mb-6">
          <h2 className="font-display text-3xl font-black mb-3 glow-text uppercase tracking-wider">
            ⚠️ RPC Configuration Required
          </h2>
          <p className="font-mono-tech text-sm text-[var(--theme-text-secondary)] leading-relaxed">
            To avoid CORS issues in browser mode, you must configure at least one HTTPS RPC endpoint 
            and one WSS WebSocket endpoint. Public Solana RPCs typically block browser requests.
          </p>
        </div>

        {/* Information Box */}
        <div className="mb-6 p-4 bg-blue-900/20 electric-border">
          <h3 className="font-mono-tech font-bold text-sm mb-2 text-blue-400">
            💡 Recommended Providers
          </h3>
          <ul className="font-mono-tech text-xs text-[var(--theme-text-secondary)] space-y-1">
            <li>• Helius: https://mainnet.helius-rpc.com/?api-key=YOUR_KEY</li>
            <li>• QuickNode: https://your-endpoint.quiknode.pro/YOUR_KEY/</li>
            <li>• Alchemy: https://solana-mainnet.g.alchemy.com/v2/YOUR_KEY</li>
            <li className="mt-2">Remember to use <code className="text-[var(--theme-accent)]">wss://</code> for WebSocket URLs!</li>
          </ul>
        </div>

        {error && (
          <div className="mb-4 p-3 bg-red-900/20 border border-red-500/50 rounded">
            <p className="font-mono-tech text-sm text-red-400">{error}</p>
          </div>
        )}

        {/* HTTPS RPC URLs */}
        <div className="mb-6">
          <label className="font-mono-tech font-bold text-sm mb-2 block uppercase tracking-wider text-[var(--theme-accent)]">
            HTTPS RPC URLs (at least 1 required)
          </label>
          <div className="flex gap-2 mb-3">
            <input
              type="text"
              value={httpsUrl}
              onChange={(e) => setHttpsUrl(e.target.value)}
              onKeyPress={(e) => e.key === 'Enter' && addHttpsUrl()}
              placeholder="https://..."
              className="flex-1 font-mono-tech text-sm p-3 bg-black electric-border focus:outline-none focus:ring-2 focus:ring-[var(--theme-accent)]"
            />
            <button
              onClick={addHttpsUrl}
              className="btn-primary px-6 py-3 font-mono-tech font-bold text-sm uppercase tracking-wider"
            >
              Add
            </button>
          </div>
          <div className="space-y-2">
            {httpsUrls.map((url, index) => (
              <div key={index} className="flex items-center gap-2 p-2 bg-black/50 electric-border">
                <span className="flex-1 font-mono-tech text-xs text-[var(--theme-text-secondary)] truncate">
                  {url}
                </span>
                <button
                  onClick={() => removeHttpsUrl(index)}
                  className="text-red-400 hover:text-red-300 font-mono-tech text-xs px-2"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        </div>

        {/* WSS WebSocket URLs */}
        <div className="mb-6">
          <label className="font-mono-tech font-bold text-sm mb-2 block uppercase tracking-wider text-[var(--theme-accent)]">
            WSS WebSocket URLs (at least 1 required)
          </label>
          <div className="flex gap-2 mb-3">
            <input
              type="text"
              value={wssUrl}
              onChange={(e) => setWssUrl(e.target.value)}
              onKeyPress={(e) => e.key === 'Enter' && addWssUrl()}
              placeholder="wss://..."
              className="flex-1 font-mono-tech text-sm p-3 bg-black electric-border focus:outline-none focus:ring-2 focus:ring-[var(--theme-accent)]"
            />
            <button
              onClick={addWssUrl}
              className="btn-primary px-6 py-3 font-mono-tech font-bold text-sm uppercase tracking-wider"
            >
              Add
            </button>
          </div>
          <div className="space-y-2">
            {wssUrls.map((url, index) => (
              <div key={index} className="flex items-center gap-2 p-2 bg-black/50 electric-border">
                <span className="flex-1 font-mono-tech text-xs text-[var(--theme-text-secondary)] truncate">
                  {url}
                </span>
                <button
                  onClick={() => removeWssUrl(index)}
                  className="text-red-400 hover:text-red-300 font-mono-tech text-xs px-2"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        </div>

        {/* Test Results */}
        {(testResults.https || testResults.wss) && (
          <div className="mb-6 p-4 bg-black/50 electric-border">
            <h3 className="font-mono-tech font-bold text-sm mb-2">Test Results:</h3>
            {testResults.https && (
              <p className="font-mono-tech text-xs mb-1">
                HTTPS: <span className={testResults.https.includes('✓') ? 'text-green-400' : 'text-red-400'}>
                  {testResults.https}
                </span>
              </p>
            )}
            {testResults.wss && (
              <p className="font-mono-tech text-xs">
                WSS: <span className={testResults.wss.includes('✓') ? 'text-green-400' : 'text-red-400'}>
                  {testResults.wss}
                </span>
              </p>
            )}
          </div>
        )}

        {/* Actions */}
        <div className="flex gap-3">
          <button
            onClick={testConnections}
            disabled={testing || !canContinue}
            className="flex-1 btn-secondary px-6 py-3 font-mono-tech font-bold text-sm uppercase tracking-wider disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {testing ? 'Testing...' : 'Test Connection'}
          </button>
          <button
            onClick={saveAndContinue}
            disabled={!canContinue}
            className="flex-1 btn-primary px-6 py-3 font-mono-tech font-bold text-sm uppercase tracking-wider disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Save & Continue
          </button>
        </div>

        <p className="font-mono-tech text-xs text-[var(--theme-text-secondary)] mt-4 text-center">
          You can update these settings later in the Configuration panel
        </p>
      </div>
    </div>
  )
}
