import React, { useEffect, useState } from 'react'
import { useWasmStore } from '../store/wasmStore'
import { apiClient } from '../services/api'
import { Play, Square, Server, Zap, Globe } from 'lucide-react'

const ModeSwitcher: React.FC = () => {
  const [isGhPages, setIsGhPages] = useState(false)
  const [isLocal, setIsLocal] = useState(false)
  const [backendAvailable, setBackendAvailable] = useState(false)
  const [isWasmRunning, setIsWasmRunning] = useState(false)
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
      setIsWasmRunning(true)
      await startBot(2000)
    } catch (e) {
      setIsWasmRunning(false)
      console.error('start failed', e)
    }
  }

  const handleStopWasm = async () => {
    try {
      setIsWasmRunning(false)
      await stopBot()
    } catch (e) {
      console.error('stop failed', e)
    }
  }

  const getEnvironmentBadge = () => {
    if (isGhPages) {
      return (
        <div className="badge badge-outline badge-info gap-1">
          <Globe className="w-3 h-3" />
          gh-pages
        </div>
      )
    } else if (isLocal) {
      return (
        <div className="badge badge-outline badge-success gap-1">
          <Server className="w-3 h-3" />
          local
        </div>
      )
    } else {
      return (
        <div className="badge badge-outline badge-warning gap-1">
          <Zap className="w-3 h-3" />
          remote
        </div>
      )
    }
  }

  return (
    <div className="card bg-base-200/50 shadow-xl border border-primary/20">
      <div className="card-body p-4">
        <h3 className="card-title text-primary text-sm mb-3">Execution Modes</h3>
        
        {/* Environment Indicator */}
        <div className="flex justify-between items-center mb-3">
          <span className="text-xs text-base-content/60 uppercase tracking-wider">Environment:</span>
          {getEnvironmentBadge()}
        </div>

        {/* WASM Mode */}
        <div className="form-control mb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <div className="badge badge-sm badge-outline">WASM</div>
              <span className="text-xs text-base-content/60">Browser-based</span>
            </div>
            <div className={`badge badge-sm ${isWasmRunning ? 'badge-success' : 'badge-ghost'}`}>
              {isWasmRunning ? 'running' : 'stopped'}
            </div>
          </div>
          <div className="join mt-2">
            <button
              onClick={handleStartWasm}
              disabled={isWasmRunning}
              className={`btn btn-sm join-item btn-primary ${isWasmRunning ? 'btn-disabled' : ''}`}
            >
              <Play className="w-3 h-3" />
              Start
            </button>
            <button
              onClick={handleStopWasm}
              disabled={!isWasmRunning}
              className={`btn btn-sm join-item btn-ghost ${!isWasmRunning ? 'btn-disabled' : ''}`}
            >
              <Square className="w-3 h-3" />
              Stop
            </button>
          </div>
        </div>

        {/* Frontend+Backend Mode */}
        <div className="form-control mb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <div className="badge badge-sm badge-outline">Full Stack</div>
              <span className="text-xs text-base-content/60">Frontend + Backend</span>
            </div>
            <div className={`badge badge-sm ${backendAvailable ? 'badge-success' : 'badge-error'}`}>
              {backendAvailable ? 'online' : 'offline'}
            </div>
          </div>
          <button
            disabled={!backendAvailable}
            className={`btn btn-sm mt-2 ${backendAvailable ? 'btn-success' : 'btn-disabled'}`}
          >
            <Server className="w-3 h-3 mr-1" />
            {backendAvailable ? 'Available' : 'Unavailable'}
          </button>
        </div>

        {/* Dedicated Mode */}
        <div className="form-control">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <div className="badge badge-sm badge-outline">Dedicated</div>
              <span className="text-xs text-base-content/60">Server deployment</span>
            </div>
            <div className="badge badge-sm badge-ghost">coming soon</div>
          </div>
          <button
            disabled
            className="btn btn-sm mt-2 btn-ghost"
          >
            <Zap className="w-3 h-3 mr-1" />
            Coming Soon
          </button>
        </div>
      </div>
    </div>
  )
}

export default ModeSwitcher
