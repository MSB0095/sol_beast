import { useCallback, useEffect, useState } from 'react'
import { AlertTriangle, CheckCircle2, Info, Skull, X } from 'lucide-react'
import { api, ApiError, storeToken, tokenFromStorage } from './api'
import { Login } from './components/Login'
import { Shell, PAGE_META, type PageId } from './components/Shell'
import { Button, Modal, TextArea } from './components/ui'
import { useDashboard, usePools, useVenues } from './hooks/useBeast'
import { Overview } from './pages/Overview'
import { RadarPage } from './pages/Radar'
import { ExecutionPage } from './pages/Execution'
import { RentPage } from './pages/Rent'
import { VenuesPage } from './pages/Venues'
import { PoolsPage } from './pages/Pools'
import { StrategiesPage } from './pages/Strategies'
import { SettingsPage } from './pages/Settings'
import type { Mode } from './types'
import { cx } from './utils'

const VALID_PAGES = Object.keys(PAGE_META) as PageId[]

function pageFromHash(): PageId {
  const value = window.location.hash.replace(/^#\/?/, '')
  return VALID_PAGES.includes(value as PageId) ? (value as PageId) : 'overview'
}

interface Toast {
  id: number
  message: string
  tone: 'good' | 'bad' | 'info'
}

export default function App() {
  const [token, setToken] = useState(() => tokenFromStorage())
  if (!token) {
    return (
      <Login
        onLogin={(value) => {
          storeToken(value)
          setToken(value)
        }}
      />
    )
  }
  return (
    <AuthenticatedApp
      token={token}
      onLogout={() => {
        storeToken('')
        setToken('')
      }}
    />
  )
}

function AuthenticatedApp({ token, onLogout }: { token: string; onLogout: () => void }) {
  const [page, setPage] = useState<PageId>(() => pageFromHash())
  const [toasts, setToasts] = useState<Toast[]>([])
  const [refreshing, setRefreshing] = useState(false)
  const [killOpen, setKillOpen] = useState(false)
  const [killReason, setKillReason] = useState('manual halt from dashboard')
  const [killing, setKilling] = useState(false)

  const notify = useCallback((message: string, tone: 'good' | 'bad' | 'info' = 'info') => {
    const id = Date.now() + Math.random()
    setToasts((current) => [...current.slice(-3), { id, message, tone }])
    window.setTimeout(() => {
      setToasts((current) => current.filter((toast) => toast.id !== id))
    }, 4_500)
  }, [])

  const unauthorized = useCallback(() => {
    storeToken('')
    onLogout()
  }, [onLogout])

  const dashboard = useDashboard(token, unauthorized)
  const venues = useVenues(token, unauthorized)
  const pools = usePools(token, unauthorized)

  useEffect(() => {
    const onHash = () => setPage(pageFromHash())
    window.addEventListener('hashchange', onHash)
    return () => window.removeEventListener('hashchange', onHash)
  }, [])

  useEffect(() => {
    if (dashboard.error) notify(dashboard.error, 'bad')
  }, [dashboard.error, notify])

  useEffect(() => {
    if (venues.error) notify(venues.error, 'bad')
  }, [venues.error, notify])

  useEffect(() => {
    if (pools.error) notify(pools.error, 'bad')
  }, [pools.error, notify])

  const navigate = (next: PageId) => {
    setPage(next)
    if (window.location.hash !== `#${next}`) {
      window.history.replaceState(null, '', `#${next}`)
    }
  }

  const refreshAll = useCallback(async () => {
    setRefreshing(true)
    try {
      await Promise.all([dashboard.refresh(true), venues.refresh(), pools.refresh()])
    } finally {
      setRefreshing(false)
    }
  }, [dashboard, pools, venues])

  const changeMode = async (mode: Mode) => {
    try {
      await api.setMode(token, mode)
      notify(`Process mode set to ${mode}`, mode === 'live' ? 'bad' : 'good')
      await dashboard.refresh(true)
    } catch (error) {
      notify(error instanceof ApiError ? error.message : 'mode change rejected', 'bad')
    }
  }

  const resume = async () => {
    try {
      await api.resume(token)
      notify('Kill switch cleared', 'good')
      await dashboard.refresh(true)
    } catch (error) {
      notify(error instanceof Error ? error.message : 'resume failed', 'bad')
    }
  }

  const kill = async () => {
    setKilling(true)
    try {
      await api.kill(token, killReason)
      notify('Kill switch activated', 'bad')
      setKillOpen(false)
      await dashboard.refresh(true)
    } catch (error) {
      notify(error instanceof Error ? error.message : 'kill switch failed', 'bad')
    } finally {
      setKilling(false)
    }
  }

  const pageBody = (() => {
    switch (page) {
      case 'overview':
        return <Overview data={dashboard.data} history={dashboard.history} onNavigate={navigate} />
      case 'radar':
        return <RadarPage data={dashboard.data} token={token} onRefresh={() => void dashboard.refresh(true)} />
      case 'execution':
        return <ExecutionPage data={dashboard.data} onRefresh={() => void dashboard.refresh(true)} />
      case 'rent':
        return (
          <RentPage
            data={dashboard.data}
            token={token}
            notify={notify}
            onRefresh={() => void dashboard.refresh(true)}
          />
        )
      case 'venues':
        return <VenuesPage data={venues.data} onRefresh={() => void venues.refresh()} />
      case 'pools':
        return (
          <PoolsPage
            token={token}
            data={pools.data}
            dashboard={dashboard.data}
            venues={venues.data?.venues || []}
            refresh={() => {
              void pools.refresh()
              void dashboard.refresh(true)
            }}
            notify={notify}
          />
        )
      case 'strategies':
        return (
          <StrategiesPage
            data={dashboard.data}
            token={token}
            notify={notify}
            onRefresh={() => void dashboard.refresh(true)}
            onNavigate={navigate}
          />
        )
      case 'settings':
        return (
          <SettingsPage
            data={dashboard.data}
            token={token}
            lastUpdated={dashboard.lastUpdated}
            notify={notify}
            onRefresh={() => void dashboard.refresh(true)}
            onLogout={onLogout}
          />
        )
      default:
        return null
    }
  })()

  return (
    <>
      <Shell
        page={page}
        onNavigate={navigate}
        data={dashboard.data}
        lastUpdated={dashboard.lastUpdated}
        onRefresh={() => void refreshAll()}
        refreshing={refreshing || dashboard.loading}
        onKill={() => setKillOpen(true)}
        onResume={resume}
        onMode={changeMode}
        onLogout={onLogout}
      >
        {pageBody}
      </Shell>

      <div className="toast-stack">
        {toasts.map((toast) => (
          <div key={toast.id} className={cx('toast', `toast-${toast.tone}`)}>
            {toast.tone === 'good' ? <CheckCircle2 size={16} /> : toast.tone === 'bad' ? <AlertTriangle size={16} /> : <Info size={16} />}
            <span>{toast.message}</span>
            <button onClick={() => setToasts((current) => current.filter((item) => item.id !== toast.id))}><X size={13} /></button>
          </div>
        ))}
      </div>

      <Modal
        open={killOpen}
        onClose={() => setKillOpen(false)}
        title="Activate kill switch"
        subtitle="This immediately blocks signing, rent recovery and live execution"
        footer={
          <>
            <Button variant="ghost" onClick={() => setKillOpen(false)}>Cancel</Button>
            <Button variant="danger" loading={killing} onClick={kill}><Skull size={14} /> Halt everything</Button>
          </>
        }
      >
        <div className="confirm-body">
          <div className="confirm-warning">
            <AlertTriangle size={18} />
            <span>The risk engine will halt, a kill-switch file is written, and the API rejects live and rent requests until you resume.</span>
          </div>
          <TextArea label="Reason (journaled)" value={killReason} onChange={(event) => setKillReason(event.target.value)} />
        </div>
      </Modal>
    </>
  )
}
