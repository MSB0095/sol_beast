import React, { useEffect, useState } from 'react'
import { useWasmStore } from '../store/wasmStore'
import { apiClient } from '../services/api'

const ModeSwitcher: React.FC = () => {
  const [isGhPages, setIsGhPages] = useState(false)
  const [isLocal, setIsLocal] = useState(false)
  const [backendAvailable, setBackendAvailable] = useState(false)
  const { startBot, stopBot } = useWasmStore()

  useEffect(() => {
    // Detect environment
    try {
      const host = window.location.hostname
      const path = window.location.pathname
      setIsGhPages(host.includes('github.io') || path.includes('/sol_beast'))
      setIsLocal(host === 'localhost' || host === '127.0.0.1')
    } catch (e) {
      setIsGhPages(false)
      setIsLocal(false)
    }

    // Check backend health
    apiClient.checkHealth().then(res => {
      if (res && res.data && res.data.status === 'ok') setBackendAvailable(true)
    }).catch(() => setBackendAvailable(false))
  }, [])

  const handleStartWasm = async () => {
    try {
      await startBot(2000)
    } catch (e) {
      console.error('start failed', e)
    }
  }

  const handleStopWasm = async () => {
    try {
      await stopBot()
    } catch (e) {
      console.error('stop failed', e)
    }
  }

  return (
    <div className="mode-switcher inline-flex items-center gap-4">
      <div className="mode-item">
        <div className="text-xs">WASM</div>
        <button onClick={handleStartWasm} className="btn btn-sm btn-success">Start WASM</button>
        <button onClick={handleStopWasm} className="btn btn-sm btn-ghost">Stop</button>
      </div>
      <div className="mode-item">
        <div className="text-xs">Frontend+Backend</div>
        <button disabled={!backendAvailable} className={`btn btn-sm ${backendAvailable ? 'btn-success' : 'btn-disabled'}`}>Available</button>
      </div>
      <div className="mode-item">
        <div className="text-xs">Dedicated</div>
        <button disabled className="btn btn-sm btn-ghost">Coming Soon</button>
      </div>
      <div className="ap-host text-xs opacity-60">{isGhPages ? 'Hosted: gh-pages' : isLocal ? 'Local' : 'Remote'}</div>
    </div>
  )
}

export default ModeSwitcher
