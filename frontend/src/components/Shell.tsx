import { useState, type ReactNode } from 'react'
import {
  Activity,
  BarChart3,
  ChevronRight,
  Coins,
  Fingerprint,
  LayoutDashboard,
  LogOut,
  Menu,
  Network,
  Radar,
  RefreshCw,
  Settings2,
  Shield,
  Skull,
  SlidersHorizontal,
  Waves,
  X,
  Zap,
} from 'lucide-react'
import type { Dashboard, Mode } from '../types'
import { cx, duration, modeLabel, modeRank, timeAgo } from '../utils'
import { Badge, Button } from './ui'

export type PageId =
  | 'overview'
  | 'radar'
  | 'execution'
  | 'rent'
  | 'venues'
  | 'pools'
  | 'strategies'
  | 'settings'

export const PAGE_META: Record<PageId, { title: string; subtitle: string }> = {
  overview: { title: 'Command center', subtitle: 'Live engine telemetry and radar pulse' },
  radar: { title: 'Radar', subtitle: 'Detected routes, modeled edge and competitor outcomes' },
  execution: { title: 'Execution', subtitle: 'Attempts, bundles, settlements and event stream' },
  rent: { title: 'Rent reclaimer', subtitle: 'Close empty token accounts and recover lamports' },
  venues: { title: 'Venues', subtitle: 'Every registered Solana DEX, AMM and liquidity venue' },
  pools: { title: 'Pool feed', subtitle: 'Watched pools, decoders and reserve freshness' },
  strategies: { title: 'Strategies', subtitle: 'Arm, cap and promote each strategy independently' },
  settings: { title: 'System', subtitle: 'Configuration, health, auth and operational controls' },
}

const NAV: Array<{ group: string; items: Array<{ id: PageId; label: string; icon: ReactNode }> }> = [
  {
    group: 'Operate',
    items: [
      { id: 'overview', label: 'Overview', icon: <LayoutDashboard size={17} /> },
      { id: 'radar', label: 'Radar', icon: <Radar size={17} /> },
      { id: 'execution', label: 'Execution', icon: <Activity size={17} /> },
      { id: 'rent', label: 'Rent reclaimer', icon: <Coins size={17} /> },
    ],
  },
  {
    group: 'Market',
    items: [
      { id: 'venues', label: 'Venues', icon: <Network size={17} /> },
      { id: 'pools', label: 'Pool feed', icon: <Waves size={17} /> },
    ],
  },
  {
    group: 'Control',
    items: [
      { id: 'strategies', label: 'Strategies', icon: <SlidersHorizontal size={17} /> },
      { id: 'settings', label: 'System', icon: <Settings2 size={17} /> },
    ],
  },
]

function navBadge(id: PageId, data: Dashboard | null): string | null {
  if (!data) return null
  if (id === 'radar') return data.status.store.opportunities > 0 ? String(data.status.store.opportunities) : null
  if (id === 'execution') return data.status.store.attempts > 0 ? String(data.status.store.attempts) : null
  if (id === 'pools') return data.status.feed?.pools ? String(data.status.feed.pools) : null
  if (id === 'venues') return String(data.venues.count)
  return null
}

function ModeBadge({ mode }: { mode: Mode }) {
  const tone = mode === 'live' ? 'bad' : mode === 'canary' ? 'warn' : mode === 'radar' ? 'good' : 'muted'
  return (
    <Badge tone={tone} dot>
      {modeLabel(mode)}
    </Badge>
  )
}

export function Shell({
  page,
  onNavigate,
  data,
  lastUpdated,
  onRefresh,
  refreshing,
  onKill,
  onResume,
  onMode,
  onLogout,
  children,
  childrenTop,
}: {
  page: PageId
  onNavigate: (page: PageId) => void
  data: Dashboard | null
  lastUpdated: number | null
  onRefresh: () => void
  refreshing: boolean
  onKill: () => void
  onResume: () => void
  onMode: (mode: Mode) => void
  onLogout: () => void
  children: ReactNode
  childrenTop?: ReactNode
}) {
  const [mobileOpen, setMobileOpen] = useState(false)
  const status = data?.status
  const meta = PAGE_META[page]
  const healthy = status && !status.kill_switch
  const feedAge = status?.feed?.age_ms
  const maxMode = status?.configured_mode || 'radar'

  const navigate = (id: PageId) => {
    onNavigate(id)
    setMobileOpen(false)
  }

  return (
    <div className="app-shell">
      <div className="scanline" />
      <aside className={cx('sidebar', mobileOpen && 'sidebar-open')}>
        <div className="sidebar-head">
          <button className="brand" onClick={() => navigate('overview')}>
            <span className="brand-mark">
              <Fingerprint size={23} strokeWidth={1.5} />
            </span>
            <span className="brand-copy">
              <span className="brand-sol">SOL</span>
              <span className="brand-beast">BEAST</span>
              <span className="brand-version">2.0 / ENGINE</span>
            </span>
          </button>
          <button className="mobile-close" onClick={() => setMobileOpen(false)} aria-label="Close menu">
            <X size={18} />
          </button>
        </div>

        <div className="sidebar-status">
          <div className="status-line">
            <span className={cx('status-orb', healthy ? 'orb-good' : 'orb-bad')} />
            <span>{status?.kill_switch ? 'Kill switch active' : 'Engine online'}</span>
            <ModeBadge mode={status?.mode || 'off'} />
          </div>
          <div className="status-meta">
            <span>UP {duration(status?.uptime_seconds)}</span>
            <span>{feedAge === null || feedAge === undefined ? 'feed —' : `feed ${timeAgo(Date.now() - feedAge)}`}</span>
          </div>
        </div>

        <nav className="sidebar-nav">
          {NAV.map((group) => (
            <div className="nav-group" key={group.group}>
              <div className="nav-group-label">{group.group}</div>
              {group.items.map((item) => {
                const badge = navBadge(item.id, data)
                const active = page === item.id
                return (
                  <button
                    key={item.id}
                    className={cx('nav-item', active && 'nav-item-active')}
                    onClick={() => navigate(item.id)}
                  >
                    <span className="nav-icon">{item.icon}</span>
                    <span>{item.label}</span>
                    {badge && <span className="nav-badge">{badge}</span>}
                    {active && <ChevronRight size={14} className="nav-chevron" />}
                  </button>
                )
              })}
            </div>
          ))}
        </nav>

        <div className="sidebar-foot">
          <div className="sidebar-health">
            <div>
              <span className={cx('dot', status?.feed ? 'dot-good' : 'dot-muted')} />
              API
            </div>
            <div>
              <span className={cx('dot', status?.jupiter_configured ? 'dot-good' : 'dot-muted')} />
              Jupiter
            </div>
            <div>
              <span className={cx('dot', status?.jito_enabled ? 'dot-good' : 'dot-muted')} />
              Jito
            </div>
            <div>
              <span className={cx('dot', status?.guard.program_id_configured ? 'dot-good' : 'dot-warn')} />
              Guard
            </div>
          </div>
          <div className="sidebar-user">
            <div className="user-avatar">
              <Shield size={14} />
            </div>
            <div className="user-copy">
              <strong>Operator</strong>
              <span>authenticated session</span>
            </div>
            <button className="icon-btn" onClick={onLogout} title="Lock session">
              <LogOut size={15} />
            </button>
          </div>
          <div className="sidebar-build">v{status?.version || '2.0.0'} · green-or-revert</div>
        </div>
      </aside>

      {mobileOpen && <div className="mobile-scrim" onClick={() => setMobileOpen(false)} />}

      <main className="main">
        <header className="topbar">
          <div className="topbar-left">
            <button className="mobile-menu" onClick={() => setMobileOpen(true)} aria-label="Open menu">
              <Menu size={20} />
            </button>
            <div>
              <div className="breadcrumb">
                <span>SOL BEAST</span>
                <ChevronRight size={12} />
                <span>{meta.title.toUpperCase()}</span>
              </div>
              <h1 className="page-title">{meta.title}</h1>
              <p className="page-subtitle">{meta.subtitle}</p>
            </div>
          </div>
          <div className="topbar-actions">
            <div className="sync-pill" title="Telemetry refresh cadence">
              <span className={cx('dot', refreshing ? 'dot-warn pulse' : 'dot-good')} />
              {refreshing ? 'SYNCING' : lastUpdated ? `SYNC ${timeAgo(lastUpdated)}` : 'CONNECTING'}
            </div>
            <button className="icon-btn icon-btn-bordered" onClick={onRefresh} title="Refresh now">
              <RefreshCw size={16} className={refreshing ? 'animate-spin' : ''} />
            </button>
            <div className="mode-select-wrap">
              <Zap size={14} />
              <select
                className="mode-select"
                value={status?.mode || 'off'}
                onChange={(event) => onMode(event.target.value as Mode)}
                title="Process-wide mode (cannot exceed configured ceiling)"
              >
                {(['off', 'radar', 'canary', 'live'] as Mode[]).map((mode) => (
                  <option key={mode} value={mode} disabled={modeRank(mode) > modeRank(maxMode)}>
                    {modeLabel(mode)}
                  </option>
                ))}
              </select>
            </div>
            {status?.kill_switch ? (
              <Button variant="outline" size="sm" onClick={onResume}>
                <Shield size={14} /> Resume
              </Button>
            ) : (
              <Button variant="danger" size="sm" onClick={onKill} title="Halt all execution">
                <Skull size={14} /> Kill
              </Button>
            )}
          </div>
        </header>

        {status?.kill_switch && (
          <div className="banner banner-danger">
            <Skull size={17} />
            <div>
              <strong>Kill switch is active.</strong> Execution and signing are halted. Review risk reasons,
              then resume deliberately.
            </div>
          </div>
        )}
        {status && !status.real && (
          <div className="banner banner-info">
            <Shield size={17} />
            <div>
              <strong>Radar posture.</strong> The engine is detecting and modeling only. Live signing requires
              <code>--real</code>, <code>runtime.real=true</code>, a deployed guard and Jito.
            </div>
          </div>
        )}
        {status?.feed?.last_error && (
          <div className="banner banner-warn">
            <BarChart3 size={17} />
            <div>
              <strong>Feed warning.</strong> {status.feed.last_error}
            </div>
          </div>
        )}

        {childrenTop}
        <div className="content">{children}</div>
        <footer className="app-foot">
          <span>SOL BEAST 2.0 · local-first keeper engine</span>
          <span>
            <span className={cx('dot', status ? 'dot-good' : 'dot-bad')} /> API {status?.api.bind || '—'}
          </span>
          <span>NO FRONT-RUNNING · NO SANDWICHING · GUARDED BUNDLES</span>
        </footer>
      </main>
    </div>
  )
}
