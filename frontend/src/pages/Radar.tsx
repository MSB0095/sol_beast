import { useMemo, useState } from 'react'
import {
  ArrowDownUp,
  ArrowUpRight,
  Crosshair,
  Filter,
  Layers3,
  LineChart,
  RefreshCw,
  Search,
  Sparkles,
  Timer,
} from 'lucide-react'
import { api } from '../api'
import type { CostBreakdown, Dashboard, Leg, OpportunityRecord } from '../types'
import { Badge, Button, CopyButton, EmptyState, Modal, Panel, PanelHeader, StatCard } from '../components/ui'
import {
  bps,
  compact,
  cx,
  dateTime,
  lamports,
  outcomeTone,
  parseJson,
  shortAddress,
  signedSol,
  timeAgo,
} from '../utils'

type SortKey = 'time' | 'net' | 'confidence'

export function RadarPage({
  data,
  token,
  onRefresh,
}: {
  data: Dashboard | null
  token: string
  onRefresh: () => void
}) {
  const [search, setSearch] = useState('')
  const [strategy, setStrategy] = useState('all')
  const [outcome, setOutcome] = useState('all')
  const [profitableOnly, setProfitableOnly] = useState(false)
  const [sort, setSort] = useState<SortKey>('time')
  const [selected, setSelected] = useState<OpportunityRecord | null>(null)
  const [quote, setQuote] = useState<Record<string, unknown> | null>(null)
  const [quoteError, setQuoteError] = useState<string | null>(null)
  const [quoting, setQuoting] = useState(false)

  const opportunities = useMemo(() => data?.opportunities || [], [data?.opportunities])
  const strategies = useMemo(
    () => Array.from(new Set(opportunities.map((item) => item.strategy))).sort(),
    [opportunities],
  )
  const outcomes = useMemo(
    () => Array.from(new Set(opportunities.map((item) => item.outcome))).sort(),
    [opportunities],
  )

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase()
    const rows = opportunities.filter((item) => {
      if (strategy !== 'all' && item.strategy !== strategy) return false
      if (outcome !== 'all' && item.outcome !== outcome) return false
      if (profitableOnly && item.est_net_lamports <= 0) return false
      if (!needle) return true
      return (
        item.strategy.includes(needle) ||
        item.dedupe_key.toLowerCase().includes(needle) ||
        item.start_mint.toLowerCase().includes(needle) ||
        item.legs_json.toLowerCase().includes(needle)
      )
    })
    return rows.sort((left, right) => {
      if (sort === 'net') return right.est_net_lamports - left.est_net_lamports
      if (sort === 'confidence') return right.confidence - left.confidence
      return right.detected_at_ms - left.detected_at_ms
    })
  }, [opportunities, outcome, profitableOnly, search, sort, strategy])

  const best = opportunities.reduce((value, item) => Math.max(value, item.est_net_lamports), 0)
  const open = opportunities.filter((item) => item.outcome === 'open').length
  const positive = opportunities.filter((item) => item.est_net_lamports > 0).length
  const avgConfidence = opportunities.length
    ? opportunities.reduce((sum, item) => sum + item.confidence, 0) / opportunities.length
    : 0

  const inspect = (item: OpportunityRecord) => {
    setSelected(item)
    setQuote(null)
    setQuoteError(null)
  }

  const runQuote = async () => {
    if (!selected) return
    const legs = parseJson<Leg[]>(selected.legs_json, [])
    if (!legs.length) {
      setQuoteError('This route has no executable legs recorded.')
      return
    }
    setQuoting(true)
    setQuoteError(null)
    try {
      const first = legs[0]
      const result = await api.quote(token, first.mint_in, first.mint_out, first.amount_in, 50)
      setQuote(result)
    } catch (error) {
      setQuoteError(error instanceof Error ? error.message : 'quote failed')
    } finally {
      setQuoting(false)
    }
  }

  return (
    <div className="page-grid">
      <div className="stats-grid stats-grid-4">
        <StatCard label="Detected" value={compact(data?.status.metrics.opportunities || 0)} detail={`${open} open in store`} icon={<Crosshair size={18} />} tone="accent" mono />
        <StatCard label="Profitable now" value={String(positive)} detail={`of ${opportunities.length} recent rows`} icon={<Sparkles size={18} />} tone="good" mono />
        <StatCard label="Best modeled net" value={`${signedSol(best)} SOL`} detail="Before execution revalidation" icon={<LineChart size={18} />} tone={best > 0 ? 'good' : 'muted'} mono />
        <StatCard label="Mean confidence" value={`${(avgConfidence * 100).toFixed(1)}%`} detail="Freshness-weighted" icon={<Timer size={18} />} tone={avgConfidence > 0.8 ? 'good' : 'warn'} mono />
      </div>

      <Panel>
        <PanelHeader
          title="Opportunity journal"
          subtitle="Every detected cycle, modeled cost, freshness and observed outcome"
          icon={<Filter size={17} />}
          actions={
            <Button variant="ghost" size="sm" onClick={onRefresh}>
              <RefreshCw size={13} /> Refresh
            </Button>
          }
        />
        <div className="toolbar">
          <div className="search-wrap">
            <Search size={14} />
            <input
              className="search-input"
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search mint, venue, strategy or pool"
            />
          </div>
          <select className="input input-select toolbar-select" value={strategy} onChange={(event) => setStrategy(event.target.value)}>
            <option value="all">All strategies</option>
            {strategies.map((item) => <option key={item} value={item}>{item.replaceAll('_', ' ')}</option>)}
          </select>
          <select className="input input-select toolbar-select" value={outcome} onChange={(event) => setOutcome(event.target.value)}>
            <option value="all">All outcomes</option>
            {outcomes.map((item) => <option key={item} value={item}>{item.replaceAll('_', ' ')}</option>)}
          </select>
          <select className="input input-select toolbar-select" value={sort} onChange={(event) => setSort(event.target.value as SortKey)}>
            <option value="time">Newest first</option>
            <option value="net">Highest net</option>
            <option value="confidence">Confidence</option>
          </select>
          <button className={cx('toggle-pill', profitableOnly && 'toggle-pill-on')} onClick={() => setProfitableOnly((value) => !value)}>
            <ArrowDownUp size={13} /> Positive only
          </button>
        </div>

        {filtered.length ? (
          <div className="table-wrap">
            <table className="data-table radar-table">
              <thead>
                <tr>
                  <th>Detected</th>
                  <th>Strategy</th>
                  <th>Cycle</th>
                  <th>Legs</th>
                  <th className="num">Gross</th>
                  <th className="num">Net SOL</th>
                  <th className="num">Conf.</th>
                  <th>Outcome</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {filtered.map((item) => {
                  const legs = parseJson<Leg[]>(item.legs_json, [])
                  return (
                    <tr key={item.id} className="clickable" onClick={() => inspect(item)}>
                      <td>
                        <div className="cell-primary mono">{dateTime(item.detected_at_ms)}</div>
                        <div className="cell-secondary">slot {item.slot_detected} · {timeAgo(item.detected_at_ms)}</div>
                      </td>
                      <td><Badge tone="info">{item.strategy.replaceAll('_', ' ')}</Badge></td>
                      <td>
                        <div className="cell-primary mono">
                          {shortAddress(item.start_mint, 4, 4)}
                          {legs.map((leg) => <span key={leg.pool} className="route-inline-arrow"> → {shortAddress(leg.mint_out, 4, 4)}</span>)}
                        </div>
                        <div className="cell-secondary">{item.dedupe_key.slice(0, 42)}{item.dedupe_key.length > 42 ? '…' : ''}</div>
                      </td>
                      <td>
                        <div className="leg-stack">
                          {legs.map((leg, index) => (
                            <span key={`${leg.pool}-${index}`}>{leg.venue}</span>
                          ))}
                        </div>
                      </td>
                      <td className="num mono-dim">{lamports(item.est_gross_lamports)}</td>
                      <td className={cx('num mono', item.est_net_lamports >= 0 ? 'text-good' : 'text-bad')}>
                        {signedSol(item.est_net_lamports)}
                      </td>
                      <td className="num mono-dim">{(item.confidence * 100).toFixed(0)}%</td>
                      <td><Badge tone={outcomeTone(item.outcome)} dot>{item.outcome.replaceAll('_', ' ')}</Badge></td>
                      <td><ArrowUpRight size={14} className="row-arrow" /></td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState
            icon={<Crosshair size={28} />}
            title={opportunities.length ? 'No rows match those filters' : 'No opportunities yet'}
            description={
              opportunities.length
                ? 'Adjust the strategy, outcome or search filters to see the rest of the journal.'
                : 'The radar persists a row only after a route clears the strategy’s net-profit and freshness gates.'
            }
          />
        )}
      </Panel>

      <div className="grid-1-1">
        <Panel>
          <PanelHeader title="Outcome taxonomy" subtitle="How the journal classifies competitor behaviour" icon={<Layers3 size={17} />} />
          <div className="legend-grid">
            {[
              ['open', 'Still visible in the detection TTL window'],
              ['taken_same_slot', 'Spread closed in the same slot — infra-bound'],
              ['taken_next_slot', 'Closed within 1–2 slots — winnable'],
              ['taken_late', 'Persisted ≥3 slots before a taker'],
              ['never_taken', 'Persisted but not executable'],
              ['open_expired', 'Vanished without an observed taker'],
            ].map(([label, detail]) => (
              <div className="legend-item" key={label}>
                <Badge tone={outcomeTone(label)} dot>{label.replaceAll('_', ' ')}</Badge>
                <span>{detail}</span>
              </div>
            ))}
          </div>
        </Panel>
        <Panel>
          <PanelHeader title="Radar guardrails" subtitle="Why a visible spread may not be an opportunity" icon={<Filter size={17} />} />
          <div className="guardrail-list">
            <div><span>01</span><p>Freshness: pool state must be inside <code>max_feed_lag_ms</code>.</p></div>
            <div><span>02</span><p>Impact: route impact must stay under the strategy’s cap.</p></div>
            <div><span>03</span><p>Costs: tip, base fees, priority fees and ATA rent are all deducted.</p></div>
            <div><span>04</span><p>Profit: only executable when net clears <code>min_net_lamports</code>; live adds the on-chain guard.</p></div>
          </div>
        </Panel>
      </div>

      <Modal
        open={selected !== null}
        onClose={() => setSelected(null)}
        title="Opportunity forensics"
        subtitle={selected ? `${selected.strategy.replaceAll('_', ' ')} · ${selected.id}` : undefined}
        wide
        footer={
          <>
            <Button variant="ghost" onClick={() => setSelected(null)}>Close</Button>
            <Button variant="secondary" onClick={runQuote} loading={quoting} disabled={!selected}>
              <LineChart size={14} /> Re-quote first leg
            </Button>
          </>
        }
      >
        {selected && <OpportunityDetail opportunity={selected} quote={quote} quoteError={quoteError} />}
      </Modal>
    </div>
  )
}

function OpportunityDetail({
  opportunity,
  quote,
  quoteError,
}: {
  opportunity: OpportunityRecord
  quote: Record<string, unknown> | null
  quoteError: string | null
}) {
  const legs = parseJson<Leg[]>(opportunity.legs_json, [])
  const costs = parseJson<CostBreakdown>(opportunity.costs_json, {
    tip_lamports: 0,
    priority_fee_lamports: 0,
    base_fee_lamports: 0,
    rent_lamports: 0,
    guard_lamports: 0,
  })
  const meta = parseJson<Record<string, unknown>>(opportunity.meta_json, {})
  const totalCost =
    costs.tip_lamports +
    costs.priority_fee_lamports +
    costs.base_fee_lamports +
    costs.rent_lamports +
    costs.guard_lamports
  return (
    <div className="detail-grid">
      <div className="detail-metrics">
        <div><span>Modeled net</span><strong className={opportunity.est_net_lamports >= 0 ? 'text-good' : 'text-bad'}>{signedSol(opportunity.est_net_lamports)} SOL</strong></div>
        <div><span>Gross</span><strong className="mono">{lamports(opportunity.est_gross_lamports)}</strong></div>
        <div><span>Total costs</span><strong className="mono">{lamports(totalCost)}</strong></div>
        <div><span>Confidence</span><strong className="mono">{(opportunity.confidence * 100).toFixed(1)}%</strong></div>
        <div><span>TTL</span><strong>{opportunity.ttl_slots} slots</strong></div>
        <div><span>Outcome</span><Badge tone={outcomeTone(opportunity.outcome)}>{opportunity.outcome.replaceAll('_', ' ')}</Badge></div>
      </div>

      <div className="detail-section">
        <h4>Route</h4>
        <div className="route-timeline">
          {legs.map((leg, index) => (
            <div className="timeline-leg" key={`${leg.pool}-${index}`}>
              <div className="timeline-marker">{index + 1}</div>
              <div className="timeline-body">
                <div className="timeline-head">
                  <span>{leg.venue}</span>
                  <Badge tone="muted">{bps(leg.price_impact_bps)}</Badge>
                </div>
                <div className="timeline-mints">
                  <span className="mono">{shortAddress(leg.mint_in, 6, 6)}</span>
                  <span>→</span>
                  <span className="mono">{shortAddress(leg.mint_out, 6, 6)}</span>
                </div>
                <div className="timeline-meta">
                  <span>in {lamports(leg.amount_in)}</span>
                  <span>min out {lamports(leg.min_out)}</span>
                  <span>pool {shortAddress(leg.pool, 5, 5)} <CopyButton value={leg.pool} /></span>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="detail-section">
        <h4>Cost ledger (lamports)</h4>
        <div className="cost-grid">
          <span>Jito tip <strong>{lamports(costs.tip_lamports)}</strong></span>
          <span>Priority <strong>{lamports(costs.priority_fee_lamports)}</strong></span>
          <span>Base fees <strong>{lamports(costs.base_fee_lamports)}</strong></span>
          <span>ATA rent <strong>{lamports(costs.rent_lamports)}</strong></span>
          <span>Guard <strong>{lamports(costs.guard_lamports)}</strong></span>
        </div>
      </div>

      <div className="detail-section">
        <h4>Detection metadata</h4>
        <div className="json-box">
          <pre>{JSON.stringify(meta, null, 2)}</pre>
        </div>
      </div>

      {(quote || quoteError) && (
        <div className="detail-section">
          <h4>Fresh Jupiter quote</h4>
          {quoteError ? <div className="form-error">{quoteError}</div> : <div className="json-box"><pre>{JSON.stringify(quote, null, 2)}</pre></div>}
        </div>
      )}

      <div className="detail-foot">
        <span>ID {opportunity.id}</span>
        <span>Slot {opportunity.slot_detected}</span>
        <span>{dateTime(opportunity.detected_at_ms)}</span>
      </div>
    </div>
  )
}
