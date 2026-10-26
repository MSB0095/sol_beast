import { useMemo, useState } from 'react'
import {
  Database,
  Droplets,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  Waves,
  Wifi,
  WifiOff,
} from 'lucide-react'
import { api } from '../api'
import type { Dashboard, PoolsResponse, Venue } from '../types'
import { Badge, Button, CopyButton, EmptyState, Input, Modal, Panel, PanelHeader, Select, Spinner, StatCard } from '../components/ui'
import { cx, dateTime, shortAddress, timeAgo } from '../utils'

export function PoolsPage({
  token,
  data,
  dashboard,
  venues,
  refresh,
  notify,
}: {
  token: string
  data: PoolsResponse | null
  dashboard: Dashboard | null
  venues: Venue[]
  refresh: () => void
  notify: (message: string, tone?: 'good' | 'bad' | 'info') => void
}) {
  const [search, setSearch] = useState('')
  const [venueFilter, setVenueFilter] = useState('all')
  const [addOpen, setAddOpen] = useState(false)
  const [removeTarget, setRemoveTarget] = useState<string | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [refreshingFeed, setRefreshingFeed] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [form, setForm] = useState({ address: '', venue: '', decimals_a: '', decimals_b: '', fee_bps: '' })

  const pools = useMemo(() => data?.pools || [], [data?.pools])
  const poolVenues = useMemo(() => Array.from(new Set(pools.map((pool) => pool.venue))).sort(), [pools])
  const latestSlot = useMemo(() => pools.reduce((max, pool) => Math.max(max, pool.slot), 0), [pools])
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase()
    return pools.filter((pool) => {
      if (venueFilter !== 'all' && pool.venue !== venueFilter) return false
      if (!needle) return true
      return (
        pool.address.toLowerCase().includes(needle) ||
        pool.venue.toLowerCase().includes(needle) ||
        pool.mint_a.toLowerCase().includes(needle) ||
        pool.mint_b.toLowerCase().includes(needle)
      )
    })
  }, [pools, search, venueFilter])

  const refreshFeed = async () => {
    setRefreshingFeed(true)
    setError(null)
    try {
      const result = await api.refreshFeed(token)
      notify(`Feed refreshed: ${result.updated}/${result.attempted} pools in ${result.duration_ms} ms`, result.failed ? 'info' : 'good')
      refresh()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'feed refresh failed')
    } finally {
      setRefreshingFeed(false)
    }
  }

  const addPool = async () => {
    setBusy('add')
    setError(null)
    try {
      await api.addPool(token, {
        address: form.address.trim(),
        venue: form.venue,
        decimals_a: form.decimals_a ? Number(form.decimals_a) : undefined,
        decimals_b: form.decimals_b ? Number(form.decimals_b) : undefined,
        fee_bps: form.fee_bps ? Number(form.fee_bps) : undefined,
      })
      notify('Pool added to the live feed', 'good')
      setAddOpen(false)
      setForm({ address: '', venue: '', decimals_a: '', decimals_b: '', fee_bps: '' })
      refresh()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'could not add pool')
    } finally {
      setBusy(null)
    }
  }

  const removePool = async () => {
    if (!removeTarget) return
    setBusy(removeTarget)
    setError(null)
    try {
      await api.removePool(token, removeTarget)
      notify('Pool removed from feed and store', 'good')
      setRemoveTarget(null)
      refresh()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'could not remove pool')
    } finally {
      setBusy(null)
    }
  }

  const feed = dashboard?.status.feed

  return (
    <div className="page-grid">
      <div className="stats-grid stats-grid-4">
        <StatCard label="Watched pools" value={String(pools.length)} detail={feed ? `${feed.quotable} quotable` : 'feed source'} icon={<Waves size={18} />} tone="accent" mono />
        <StatCard label="Stale" value={String(feed?.stale ?? 0)} detail="Outside freshness window" icon={feed?.stale ? <WifiOff size={18} /> : <Wifi size={18} />} tone={feed?.stale ? 'warn' : 'good'} mono />
        <StatCard label="Last refresh" value={feed?.age_ms != null ? `${feed.age_ms} ms` : '—'} detail={feed?.last_refresh_ms ? dateTime(feed.last_refresh_ms) : 'not refreshed'} icon={<RefreshCw size={18} />} tone="info" mono />
        <StatCard label="Latest slot" value={latestSlot ? String(latestSlot) : '—'} detail="Maximum decoded pool slot" icon={<Database size={18} />} tone="info" mono />
      </div>

      {error && <div className="form-error">{error}</div>}

      <Panel>
        <PanelHeader
          title="Pool watchlist"
          subtitle="Fresh decoded reserves power the local router; stale pools are excluded"
          icon={<Droplets size={17} />}
          actions={
            <>
              <Button variant="ghost" size="sm" onClick={refreshFeed} loading={refreshingFeed}><RefreshCw size={13} /> Refresh feed</Button>
              <Button variant="primary" size="sm" onClick={() => { setError(null); setForm((current) => ({ ...current, venue: current.venue || venues[0]?.id || '' })); setAddOpen(true) }}><Plus size={14} /> Add pool</Button>
            </>
          }
        />
        <div className="toolbar">
          <div className="search-wrap">
            <Search size={14} />
            <input className="search-input" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search pool, mint or venue" />
          </div>
          <select className="input input-select toolbar-select" value={venueFilter} onChange={(event) => setVenueFilter(event.target.value)}>
            <option value="all">All venues</option>
            {poolVenues.map((venue) => <option key={venue} value={venue}>{venue}</option>)}
          </select>
          <span className="toolbar-count">{filtered.length} / {pools.length}</span>
        </div>

        {filtered.length ? (
          <div className="table-wrap">
            <table className="data-table">
              <thead><tr><th>Pool</th><th>Venue</th><th>Pair</th><th className="num">Fee</th><th className="num">Slot</th><th className="num">Conf.</th><th>Model</th><th /></tr></thead>
              <tbody>
                {filtered.map((pool) => {
                  const stale = latestSlot > 0 && pool.slot > 0 && latestSlot - pool.slot > 150
                  return (
                    <tr key={pool.address}>
                      <td>
                        <div className="cell-primary mono">{shortAddress(pool.address, 7, 7)}</div>
                        <CopyButton value={pool.address} />
                      </td>
                      <td><Badge tone="info">{pool.venue}</Badge></td>
                      <td>
                        <div className="cell-primary mono">{shortAddress(pool.mint_a, 4, 4)} / {shortAddress(pool.mint_b, 4, 4)}</div>
                        <div className="cell-secondary">decimals {pool.decimals_a}/{pool.decimals_b}</div>
                      </td>
                      <td className="num mono-dim">{pool.fee_bps} bps</td>
                      <td className="num"><span className={cx('mono', stale && 'text-warn')}>{pool.slot || '—'}</span></td>
                      <td className="num mono-dim">{(pool.confidence * 100).toFixed(0)}%</td>
                      <td><Badge tone={stale ? 'warn' : 'good'}>{stale ? 'stale' : 'fresh'}</Badge></td>
                      <td>
                        <button className="icon-btn danger-hover" onClick={() => setRemoveTarget(pool.address)} title="Remove pool">
                          <Trash2 size={14} />
                        </button>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            icon={<Waves size={28} />}
            title={pools.length ? 'No pools match' : 'The watchlist is empty'}
            description={
              pools.length
                ? 'Try a different venue or search term.'
                : 'Add a pool manually, or run beast discover --venue <id> --save to decode pools from chain, then restart or refresh the feed.'
            }
            action={<Button variant="secondary" size="sm" onClick={() => setAddOpen(true)}><Plus size={13} /> Add a pool</Button>}
          />
        )}
      </Panel>

      <div className="grid-1-1">
        <Panel>
          <PanelHeader title="Feed architecture" subtitle="Local reserves first, explicit staircase" icon={<Database size={17} />} />
          <div className="feed-architecture">
            <div><span>1</span><div><strong>Discover</strong><p>Decode program accounts with a verified venue layout.</p></div></div>
            <div><span>2</span><div><strong>Hydrate</strong><p>Read vault balances and mint decimals in parallel.</p></div></div>
            <div><span>3</span><div><strong>Model</strong><p>CPMM, CLMM, DLMM, stableswap or weighted math.</p></div></div>
            <div><span>4</span><div><strong>Expire</strong><p>Drop stale pools from routing before a quote is built.</p></div></div>
          </div>
        </Panel>
        <Panel>
          <PanelHeader title="Current feed" subtitle="Health reported by the engine" icon={<Wifi size={17} />} actions={feed ? <Badge tone={feed.stale ? 'warn' : 'good'} dot>{feed.stale ? 'degraded' : 'fresh'}</Badge> : undefined} />
          {feed ? (
            <div className="feed-health">
              <div><span>Pools</span><strong>{feed.pools}</strong></div>
              <div><span>Quotable</span><strong>{feed.quotable}</strong></div>
              <div><span>Stale</span><strong className={feed.stale ? 'text-warn' : ''}>{feed.stale}</strong></div>
              <div><span>Last refresh</span><strong>{feed.last_refresh_ms ? timeAgo(feed.last_refresh_ms) : 'never'}</strong></div>
            </div>
          ) : <EmptyState compact icon={<WifiOff size={22} />} title="Feed unavailable" description="The API is running without a pool feed." />}
          {feed?.last_error && <div className="form-error">{feed.last_error}</div>}
          <div className="feed-tip">
            <code>beast discover --venue raydium_amm --limit 50 --save</code>
            <span>Discovery writes decoded snapshots into SQLite; the feed reloads them at startup.</span>
          </div>
        </Panel>
      </div>

      <Modal
        open={addOpen}
        onClose={() => setAddOpen(false)}
        title="Add pool to watchlist"
        subtitle="The feed will hydrate reserves and mints on the next refresh"
        footer={
          <>
            <Button variant="ghost" onClick={() => setAddOpen(false)}>Cancel</Button>
            <Button variant="primary" loading={busy === 'add'} onClick={() => void addPool()}>Add pool</Button>
          </>
        }
      >
        <form className="form-grid" onSubmit={(event) => { event.preventDefault(); void addPool() }}>
          <Input label="Pool address" value={form.address} onChange={(event) => setForm({ ...form, address: event.target.value })} placeholder="base58 pool account" required />
          <Select label="Venue" value={form.venue} onChange={(event) => setForm({ ...form, venue: event.target.value })} required>
            <option value="">Select a registered venue</option>
            {venues.map((venue) => <option key={venue.id} value={venue.id}>{venue.name} ({venue.coverage})</option>)}
          </Select>
          <div className="form-row">
            <Input label="Mint A decimals" type="number" min="0" max="18" value={form.decimals_a} onChange={(event) => setForm({ ...form, decimals_a: event.target.value })} hint="Optional; auto-detected" />
            <Input label="Mint B decimals" type="number" min="0" max="18" value={form.decimals_b} onChange={(event) => setForm({ ...form, decimals_b: event.target.value })} hint="Optional; auto-detected" />
          </div>
          <Input label="Fee override (bps)" type="number" min="0" max="1000" value={form.fee_bps} onChange={(event) => setForm({ ...form, fee_bps: event.target.value })} hint="Optional; venue default is used when blank" />
          {error && <div className="form-error">{error}</div>}
        </form>
      </Modal>

      <Modal
        open={removeTarget !== null}
        onClose={() => setRemoveTarget(null)}
        title="Remove pool"
        subtitle="The pool is removed from the in-memory feed and local store"
        footer={
          <>
            <Button variant="ghost" onClick={() => setRemoveTarget(null)}>Cancel</Button>
            <Button variant="danger" loading={busy === removeTarget} onClick={removePool}><Trash2 size={14} /> Remove</Button>
          </>
        }
      >
        <p className="confirm-copy">Remove <code>{removeTarget ? shortAddress(removeTarget, 8, 8) : ''}</code> from the watchlist? Existing radar rows are retained for audit.</p>
      </Modal>

      {!data && <Spinner label="Loading pool feed" />}
    </div>
  )
}
