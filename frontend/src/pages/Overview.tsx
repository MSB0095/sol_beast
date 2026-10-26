import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'
import {
  Activity,
  ArrowUpRight,
  Boxes,
  CircleDollarSign,
  Crosshair,
  Cpu,
  Gauge,
  Layers,
  LockKeyhole,
  Radar,
  Recycle,
  ShieldCheck,
  Sparkles,
  TrendingUp,
  Wallet,
  Waves,
  Zap,
} from 'lucide-react'
import type { Dashboard } from '../types'
import type { HistoryPoint } from '../hooks/useBeast'
import type { PageId } from '../components/Shell'
import { Badge, Button, EmptyState, MiniBar, Panel, PanelHeader, StatCard } from '../components/ui'
import {
  compact,
  coverageLabel,
  cx,
  dateTime,
  lamports,
  modeLabel,
  outcomeTone,
  parseJson,
  shortAddress,
  signedSol,
  sol,
  statusTone,
  timeAgo,
} from '../utils'

function ChartTip({
  active,
  payload,
  label,
}: {
  active?: boolean
  payload?: Array<{ name?: string; value?: number; color?: string; dataKey?: string }>
  label?: string
}) {
  if (!active || !payload?.length) return null
  return (
    <div className="chart-tip">
      <span>{label}</span>
      {payload.map((entry) => (
        <div key={entry.dataKey || entry.name}>
          <i style={{ background: entry.color }} />
          <span>{entry.name}</span>
          <strong>
            {entry.dataKey === 'realized'
              ? `${signedSol(entry.value || 0)} SOL`
              : compact(entry.value || 0)}
          </strong>
        </div>
      ))}
    </div>
  )
}

export function Overview({
  data,
  history,
  onNavigate,
}: {
  data: Dashboard | null
  history: HistoryPoint[]
  onNavigate: (page: PageId) => void
}) {
  if (!data) {
    return (
      <Panel>
        <EmptyState
          icon={<Cpu size={28} />}
          title="Connecting to the engine"
          description="Waiting for the first authenticated telemetry snapshot."
        />
      </Panel>
    )
  }

  const { status } = data
  const metrics = status.metrics
  const best = data.opportunities.reduce<Dashboard['opportunities'][number] | null>(
    (best, item) => (!best || item.est_net_lamports > best.est_net_lamports ? item : best),
    null,
  )
  const landRate =
    metrics.bundles_submitted > 0 ? metrics.bundles_landed / metrics.bundles_submitted : 0
  const liveReady =
    status.signing_enabled &&
    status.jito_enabled &&
    status.guard.enabled &&
    status.guard.mode === 'program' &&
    status.guard.program_id_configured &&
    status.wallet.keypair_configured
  const riskState = status.risk?.state || 'unknown'
  const riskTone =
    riskState === 'healthy' || riskState === 'ok' ? 'good' : riskState === 'degraded' ? 'warn' : 'bad'
  const latestSettlement = data.settlements[0]
  const chartData = history.length ? history : [{ t: 'now', opportunities: metrics.opportunities, modeled: 0, realized: metrics.realized_net_lamports, bundles: 0, landed: 0 }]
  const coverageEntries = Object.entries(data.venues.coverage).sort((a, b) => b[1] - a[1])
  const topStrategies = [...data.strategies_summary].sort(
    (left, right) => right.modeled_net_lamports - left.modeled_net_lamports,
  )

  return (
    <div className="page-grid">
      <section className="hero">
        <div className="hero-glow" />
        <div className="hero-main">
          <div className="eyebrow">
            <span className="live-dot" /> MISSION CONTROL / {status.real ? 'LIVE-CAPABLE' : 'RADAR POSTURE'}
          </div>
          <h2>
            Every submitted bundle clears <span>the guard</span> — or reverts.
          </h2>
          <p>
            Detect cross-venue dislocations, model every lamport of cost, then submit a bundle only when
            the on-chain guard can prove net SOL profit. No sandwiches. No front-running. No unguarded
            bundles.
          </p>
          <div className="hero-actions">
            <Button variant="primary" onClick={() => onNavigate('radar')}>
              <Radar size={15} /> Open radar
            </Button>
            <Button variant="outline" onClick={() => onNavigate('rent')}>
              <Recycle size={15} /> Reclaim rent
            </Button>
            <Button variant="ghost" onClick={() => onNavigate('venues')}>
              <Layers size={15} /> {data.venues.count} venues
            </Button>
          </div>
        </div>
        <div className="hero-side">
          <div className="hero-net">
            <span>REALIZED NET</span>
            <strong className={metrics.realized_net_lamports >= 0 ? 'text-good' : 'text-bad'}>
              {signedSol(metrics.realized_net_lamports)}
            </strong>
            <em>SOL</em>
          </div>
          <div className="hero-checks">
            <div>
              <ShieldCheck size={15} />
              <span>Guard</span>
              <strong>{status.guard.program_id_configured ? 'ARMED' : 'UNSET'}</strong>
            </div>
            <div>
              <LockKeyhole size={15} />
              <span>Live readiness</span>
              <strong>{liveReady ? 'READY' : 'GATED'}</strong>
            </div>
            <div>
              <Gauge size={15} />
              <span>Mode</span>
              <strong>{modeLabel(status.mode).toUpperCase()}</strong>
            </div>
          </div>
        </div>
      </section>

      <div className="stats-grid">
        <StatCard
          label="Realized net"
          value={`${signedSol(metrics.realized_net_lamports)} SOL`}
          detail={
            latestSettlement
              ? `Last settlement ${timeAgo(latestSettlement.settled_at_ms)}`
              : 'No on-chain settlement yet'
          }
          icon={<CircleDollarSign size={18} />}
          tone={metrics.realized_net_lamports >= 0 ? 'good' : 'bad'}
          mono
        />
        <StatCard
          label="Opportunities"
          value={compact(metrics.opportunities)}
          detail={`${data.opportunities.filter((item) => item.est_net_lamports > 0).length} profitable in recent window`}
          icon={<Radar size={18} />}
          tone="accent"
          mono
        />
        <StatCard
          label="Bundle land rate"
          value={metrics.bundles_submitted ? `${(landRate * 100).toFixed(1)}%` : '—'}
          detail={`${metrics.bundles_landed} landed / ${metrics.bundles_submitted} submitted`}
          icon={<Boxes size={18} />}
          tone={landRate >= 0.8 ? 'good' : landRate > 0 ? 'warn' : 'muted'}
          mono
        />
        <StatCard
          label="Feed coverage"
          value={status.feed ? `${status.feed.quotable}/${status.feed.pools}` : '—'}
          detail={
            status.feed
              ? `${status.feed.stale} stale · ${status.feed.age_ms ? `${status.feed.age_ms} ms age` : 'awaiting data'}`
              : 'Feed not configured'
          }
          icon={<Waves size={18} />}
          tone={status.feed && status.feed.stale === 0 && status.feed.pools > 0 ? 'good' : 'warn'}
          mono
        />
        <StatCard
          label="Rent recovered"
          value={`${sol(metrics.rent_recovered_lamports, 4)} SOL`}
          detail={`${metrics.rent_closed_accounts} empty accounts closed`}
          icon={<Recycle size={18} />}
          tone="good"
          mono
        />
        <StatCard
          label="Minimum profit"
          value={`${sol(status.guard.min_profit_lamports, 6)} SOL`}
          detail={`${lamports(status.guard.min_profit_lamports)} lamports · risk ${riskState}`}
          icon={<ShieldCheck size={18} />}
          tone={riskTone}
          mono
        />
      </div>

      <div className="grid-2-1">
        <Panel className="chart-panel">
          <PanelHeader
            title="Radar pulse"
            subtitle="Cumulative detections and realized net across the session"
            icon={<Activity size={17} />}
            actions={
              <>
                <div className="legend">
                  <span><i className="legend-dot legend-accent" /> Detections</span>
                  <span><i className="legend-dot legend-good" /> Realized SOL</span>
                </div>
                <Badge tone="good" dot>{status.metrics.feed_updates} feed updates</Badge>
              </>
            }
          />
          <div className="chart-wrap">
            <ResponsiveContainer width="100%" height={230}>
              <ComposedChart data={chartData} margin={{ top: 10, right: 12, left: -18, bottom: 0 }}>
                <defs>
                  <linearGradient id="radarFill" x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor="#b8ff3d" stopOpacity={0.32} />
                    <stop offset="100%" stopColor="#b8ff3d" stopOpacity={0.01} />
                  </linearGradient>
                </defs>
                <CartesianGrid strokeDasharray="3 8" stroke="rgba(255,255,255,0.055)" vertical={false} />
                <XAxis dataKey="t" tick={{ fill: '#74827a', fontSize: 10 }} axisLine={false} tickLine={false} minTickGap={40} />
                <YAxis yAxisId="left" tick={{ fill: '#74827a', fontSize: 10 }} axisLine={false} tickLine={false} />
                <YAxis yAxisId="right" orientation="right" tick={{ fill: '#74827a', fontSize: 10 }} axisLine={false} tickLine={false} tickFormatter={(value) => `${(Number(value) / 1e9).toFixed(2)}`} />
                <Tooltip content={<ChartTip />} />
                <Area yAxisId="left" type="monotone" dataKey="opportunities" name="Detections" stroke="#b8ff3d" strokeWidth={2} fill="url(#radarFill)" activeDot={{ r: 4, fill: '#b8ff3d', strokeWidth: 0 }} />
                <Line yAxisId="right" type="monotone" dataKey="realized" name="Realized" stroke="#38e8a4" strokeWidth={1.6} dot={false} strokeDasharray="4 4" />
              </ComposedChart>
            </ResponsiveContainer>
          </div>
        </Panel>

        <Panel className="readiness-panel">
          <PanelHeader
            title="Engine readiness"
            subtitle="Green-or-revert execution chain"
            icon={<Zap size={17} />}
            actions={<Badge tone={liveReady ? 'good' : 'warn'} dot>{liveReady ? 'live ready' : 'gated'}</Badge>}
          />
          <div className="readiness-list">
            <ReadinessRow label="API control plane" detail={status.api.bind} ok={status.api.enabled} />
            <ReadinessRow label="Jupiter quotes" detail={status.jupiter_configured ? 'configured' : 'missing'} ok={status.jupiter_configured} />
            <ReadinessRow label="Jito bundles" detail={status.jito_enabled ? 'enabled' : 'disabled'} ok={status.jito_enabled} />
            <ReadinessRow
              label="Profit guard"
              detail={status.guard.program_id_configured ? `${status.guard.mode} / armed` : 'program id unset'}
              ok={status.guard.enabled && status.guard.program_id_configured}
            />
            <ReadinessRow label="Wallet" detail={status.wallet.keypair_configured ? 'keypair present' : 'keypair missing'} ok={status.wallet.keypair_configured} />
            <ReadinessRow label="Rent reclaimer" detail={status.rent_configured ? 'wallet ready' : 'not available'} ok={status.rent_configured} />
          </div>
          <div className="readiness-foot">
            <div className="risk-state">
              <span>Risk</span>
              <Badge tone={riskTone}>{riskState}</Badge>
            </div>
            {status.risk?.reasons.length ? (
              <p>{status.risk.reasons.map((reason) => reason.detail).join(' · ')}</p>
            ) : (
              <p>No active risk reasons. {status.real ? 'Live gates are evaluated per opportunity.' : 'Radar never signs transactions.'}</p>
            )}
          </div>
        </Panel>
      </div>

      <div className="grid-1-1">
        <Panel>
          <PanelHeader
            title="Best modeled route"
            subtitle="Highest net opportunity currently in the local store"
            icon={<Crosshair size={17} />}
            actions={best ? <Badge tone={best.est_net_lamports > 0 ? 'good' : 'bad'}>{best.strategy.replaceAll('_', ' ')}</Badge> : undefined}
          />
          {best ? (
            <div className="best-route">
              <div className="best-route-top">
                <div>
                  <span className="muted-label">MODELED NET</span>
                  <strong className={best.est_net_lamports >= 0 ? 'text-good' : 'text-bad'}>
                    {signedSol(best.est_net_lamports)} <em>SOL</em>
                  </strong>
                </div>
                <div>
                  <span className="muted-label">CONFIDENCE</span>
                  <strong>{(best.confidence * 100).toFixed(0)}%</strong>
                </div>
                <div>
                  <span className="muted-label">DETECTED</span>
                  <strong>{timeAgo(best.detected_at_ms)}</strong>
                </div>
                <Badge tone={outcomeTone(best.outcome)} dot>{best.outcome.replaceAll('_', ' ')}</Badge>
              </div>
              <div className="route-visual">
                {parseJson<Array<{ mint_in: string; mint_out: string; venue: string; pool: string }>>(
                  best.legs_json,
                  [],
                ).map((leg, index) => (
                  <div className="route-leg" key={`${leg.pool}-${index}`}>
                    <span className="route-node">{shortAddress(leg.mint_in, 4, 4)}</span>
                    <span className="route-arrow">
                      <i />
                      <em>{leg.venue}</em>
                    </span>
                    {index === parseJson<unknown[]>(best.legs_json, []).length - 1 && (
                      <span className="route-node">{shortAddress(leg.mint_out, 4, 4)}</span>
                    )}
                  </div>
                ))}
              </div>
              <div className="best-route-actions">
                <Button variant="secondary" size="sm" onClick={() => onNavigate('radar')}>
                  Inspect route <ArrowUpRight size={13} />
                </Button>
                <span className="mono-dim">slot {best.slot_detected}</span>
              </div>
            </div>
          ) : (
            <EmptyState
              compact
              icon={<Sparkles size={24} />}
              title="No route has cleared the threshold"
              description="The radar scans every fresh pool in your feed. Add pools or wait for a dislocation."
              action={<Button variant="ghost" size="sm" onClick={() => onNavigate('pools')}>Manage pool feed</Button>}
            />
          )}
        </Panel>

        <Panel>
          <PanelHeader
            title="Venue coverage"
            subtitle={`${data.venues.count} registered programs / local models first, Jupiter fallback`}
            icon={<Layers size={17} />}
            actions={<Button variant="ghost" size="sm" onClick={() => onNavigate('venues')}>All venues <ArrowUpRight size={12} /></Button>}
          />
          <div className="coverage-list">
            {coverageEntries.map(([coverage, count]) => (
              <div className="coverage-row" key={coverage}>
                <div>
                  <span className="coverage-name">{coverageLabel(coverage)}</span>
                  <span className="coverage-count">{count}</span>
                </div>
                <MiniBar value={(count / data.venues.count) * 100} tone={coverage === 'modeled' || coverage === 'stake_pool' ? 'good' : coverage === 'disabled' ? 'bad' : 'info'} />
              </div>
            ))}
          </div>
          <div className="coverage-note">
            <TrendingUp size={14} />
            <span>
              {data.venues.coverage.modeled || 0} venues use verified local math; quote-only venues route through
              Jupiter with explicit cost and stale-data checks. Catalogued and disabled venues never route.
            </span>
          </div>
        </Panel>
      </div>

      <div className="grid-2-1">
        <Panel>
          <PanelHeader
            title="Recent opportunities"
            subtitle="Newest detections from the SQLite radar journal"
            icon={<Radar size={17} />}
            actions={<Button variant="ghost" size="sm" onClick={() => onNavigate('radar')}>Open radar <ArrowUpRight size={12} /></Button>}
          />
          {data.opportunities.length ? (
            <div className="table-wrap">
              <table className="data-table">
                <thead>
                  <tr>
                    <th>Time</th>
                    <th>Strategy</th>
                    <th>Route</th>
                    <th className="num">Net SOL</th>
                    <th className="num">Conf.</th>
                    <th>Outcome</th>
                  </tr>
                </thead>
                <tbody>
                  {data.opportunities.slice(0, 8).map((item) => {
                    const legs = parseJson<Array<{ venue: string }>>(item.legs_json, [])
                    return (
                      <tr key={item.id}>
                        <td className="mono-dim">{dateTime(item.detected_at_ms)}</td>
                        <td><Badge tone="info">{item.strategy.replaceAll('_', ' ')}</Badge></td>
                        <td>
                          <div className="route-inline">
                            <span className="mono">{shortAddress(item.start_mint, 4, 4)}</span>
                            <span className="route-inline-arrow">→</span>
                            <span className="mono-dim">{legs.map((leg) => leg.venue).join(' · ') || 'quote'}</span>
                          </div>
                        </td>
                        <td className={cx('num mono', item.est_net_lamports >= 0 ? 'text-good' : 'text-bad')}>
                          {signedSol(item.est_net_lamports)}
                        </td>
                        <td className="num mono-dim">{(item.confidence * 100).toFixed(0)}%</td>
                        <td><Badge tone={outcomeTone(item.outcome)} dot>{item.outcome.replaceAll('_', ' ')}</Badge></td>
                      </tr>
                    )
                  })}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState compact icon={<Radar size={24} />} title="Radar is quiet" description="No opportunities persisted yet. Configure pools and allow the engine to scan." />
          )}
        </Panel>

        <Panel>
          <PanelHeader
            title="Execution pulse"
            subtitle="Recent attempts and settlement truth"
            icon={<Cpu size={17} />}
            actions={<Badge tone="muted">{data.attempts.length} recent</Badge>}
          />
          <div className="execution-mini">
            {data.attempts.slice(0, 5).map((attempt) => (
              <div className="exec-row" key={attempt.id}>
                <span className={cx('exec-orb', `orb-${statusTone(attempt.status)}`)} />
                <div className="exec-copy">
                  <strong>{attempt.status.replaceAll('_', ' ')}</strong>
                  <span>{attempt.strategy.replaceAll('_', ' ')} · {timeAgo(attempt.ts_ms)}</span>
                </div>
                <div className="exec-net">
                  <span className={attempt.realized_net_lamports && attempt.realized_net_lamports < 0 ? 'text-bad' : 'text-good'}>
                    {attempt.realized_net_lamports === null ? '—' : `${signedSol(attempt.realized_net_lamports)} SOL`}
                  </span>
                  <em>{lamports(attempt.tip_lamports)} tip</em>
                </div>
              </div>
            ))}
            {!data.attempts.length && (
              <EmptyState compact icon={<Activity size={22} />} title="No executions yet" description="Canary simulations and live attempts appear here with real settlement data." />
            )}
          </div>
          <div className="mini-summary">
            <div><span>Submitted</span><strong>{metrics.bundles_submitted}</strong></div>
            <div><span>Dropped</span><strong>{metrics.bundles_dropped}</strong></div>
            <div><span>Reverted</span><strong>{metrics.bundles_reverted}</strong></div>
            <div><span>Guard fails</span><strong>{metrics.guard_violations}</strong></div>
          </div>
        </Panel>
      </div>

      <div className="grid-1-1">
        <Panel>
          <PanelHeader
            title="Strategy edge"
            subtitle="Modeled net by strategy, ranked"
            icon={<Wallet size={17} />}
            actions={<Badge tone="accent">{topStrategies.length} active</Badge>}
          />
          {topStrategies.length ? (
            <div className="strategy-bars">
              {topStrategies.map((row) => {
                const max = Math.max(...topStrategies.map((item) => Math.abs(item.modeled_net_lamports)), 1)
                return (
                  <div className="strategy-bar-row" key={row.strategy}>
                    <div className="strategy-bar-label">
                      <span>{row.strategy.replaceAll('_', ' ')}</span>
                      <em>{row.opportunities} opp</em>
                    </div>
                    <div className="strategy-bar-track">
                      <span
                        className={row.modeled_net_lamports >= 0 ? 'tone-bg-good' : 'tone-bg-bad'}
                        style={{ width: `${(Math.abs(row.modeled_net_lamports) / max) * 100}%` }}
                      />
                    </div>
                    <span className={cx('strategy-bar-value', row.modeled_net_lamports >= 0 ? 'text-good' : 'text-bad')}>
                      {signedSol(row.modeled_net_lamports)}
                    </span>
                  </div>
                )
              })}
            </div>
          ) : (
            <EmptyState compact icon={<TrendingUp size={22} />} title="No strategy edge yet" description="Once the radar persists opportunities, per-strategy modeled net appears here." />
          )}
        </Panel>

        <Panel>
          <PanelHeader
            title="Event stream"
            subtitle="Operator-visible engine events"
            icon={<Activity size={17} />}
            actions={<Button variant="ghost" size="sm" onClick={() => onNavigate('settings')}>System <ArrowUpRight size={12} /></Button>}
          />
          <div className="event-list">
            {data.events.slice(0, 7).map((event) => (
              <div className="event-row" key={event.id}>
                <span className={cx('event-dot', event.level === 'error' ? 'dot-bad' : event.level === 'warn' ? 'dot-warn' : 'dot-good')} />
                <div>
                  <strong>{event.message}</strong>
                  <span>{event.component} · {timeAgo(event.ts_ms)}</span>
                </div>
              </div>
            ))}
            {!data.events.length && (
              <EmptyState compact icon={<Activity size={22} />} title="No events yet" description="The event journal records risk changes, mode changes, feed refreshes and executions." />
            )}
          </div>
        </Panel>
      </div>
    </div>
  )
}

function ReadinessRow({ label, detail, ok }: { label: string; detail: string; ok: boolean }) {
  return (
    <div className="readiness-row">
      <div>
        <span className={cx('dot', ok ? 'dot-good' : 'dot-warn')} />
        <strong>{label}</strong>
      </div>
      <span>{detail}</span>
    </div>
  )
}
