import { useState } from 'react'
import {
  Activity,
  ArrowUpRight,
  Boxes,
  CheckCircle2,
  CircleOff,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Terminal,
  Wallet,
} from 'lucide-react'
import type { Attempt, Bundle, Dashboard, Settlement } from '../types'
import { Badge, Button, CopyButton, EmptyState, Modal, Panel, PanelHeader, StatCard } from '../components/ui'
import {
  bps,
  cx,
  dateTime,
  lamports,
  parseJson,
  shortAddress,
  signedSol,
  statusTone,
  timeAgo,
} from '../utils'

type Tab = 'attempts' | 'bundles' | 'settlements' | 'events'

const TABS: Array<{ id: Tab; label: string; icon: typeof Activity }> = [
  { id: 'attempts', label: 'Attempts', icon: Activity },
  { id: 'bundles', label: 'Bundles', icon: Boxes },
  { id: 'settlements', label: 'Settlements', icon: Wallet },
  { id: 'events', label: 'Events', icon: Terminal },
]

export function ExecutionPage({ data, onRefresh }: { data: Dashboard | null; onRefresh: () => void }) {
  const [tab, setTab] = useState<Tab>('attempts')
  const [selectedAttempt, setSelectedAttempt] = useState<Attempt | null>(null)
  const [selectedBundle, setSelectedBundle] = useState<Bundle | null>(null)

  const metrics = data?.status.metrics
  const settlements = data?.settlements || []
  const averageError = settlements.length
    ? settlements.reduce((sum, row) => sum + modelErrorBps(row), 0) / settlements.length
    : 0

  return (
    <div className="page-grid">
      <div className="stats-grid stats-grid-4">
        <StatCard label="Simulated" value={String(metrics?.simulated || 0)} detail="Bundles that passed simulation" icon={<CheckCircle2 size={18} />} tone="good" mono />
        <StatCard label="Submitted" value={String(metrics?.bundles_submitted || 0)} detail={`${metrics?.bundles_landed || 0} landed`} icon={<Boxes size={18} />} tone="accent" mono />
        <StatCard label="Dropped / reverted" value={`${metrics?.bundles_dropped || 0} / ${metrics?.bundles_reverted || 0}`} detail="Dropped must cost zero" icon={<CircleOff size={18} />} tone={metrics?.bundles_reverted ? 'bad' : 'warn'} mono />
        <StatCard label="Mean model error" value={settlements.length ? bps(averageError) : '—'} detail="Realized vs modeled settlement" icon={<ShieldAlert size={18} />} tone={averageError > 500 ? 'bad' : 'good'} mono />
      </div>

      <section className="gate-strip">
        <div className="gate-item">
          <span className="gate-icon"><ShieldCheck size={17} /></span>
          <div><strong>On-chain guard</strong><span>{data?.status.guard.program_id_configured ? `${data.status.guard.mode} / armed` : 'program id unset'}</span></div>
        </div>
        <div className="gate-item">
          <span className="gate-icon"><Boxes size={17} /></span>
          <div><strong>Bundle route</strong><span>{data?.status.jito_enabled ? 'Jito block engine enabled' : 'Jito disabled'}</span></div>
        </div>
        <div className="gate-item">
          <span className="gate-icon"><Wallet size={17} /></span>
          <div><strong>Live execution</strong><span>{data?.status.signing_enabled ? 'explicitly enabled' : 'blocked — radar posture'}</span></div>
        </div>
        <div className="gate-item">
          <span className="gate-icon"><ArrowUpRight size={17} /></span>
          <div><strong>Tip policy</strong><span>tip + fee + rent + min profit in guard threshold</span></div>
        </div>
      </section>

      <Panel>
        <PanelHeader
          title="Execution ledger"
          subtitle="Nothing here is inferred from the model — settlements come from on-chain balances"
          icon={<Boxes size={17} />}
          actions={<Button variant="ghost" size="sm" onClick={onRefresh}><RefreshCw size={13} /> Refresh</Button>}
        />
        <div className="tabs">
          {TABS.map((item) => {
            const Icon = item.icon
            const count =
              item.id === 'attempts'
                ? data?.attempts.length
                : item.id === 'bundles'
                  ? data?.bundles.length
                  : item.id === 'settlements'
                    ? data?.settlements.length
                    : data?.events.length
            return (
              <button key={item.id} className={cx('tab', tab === item.id && 'tab-active')} onClick={() => setTab(item.id)}>
                <Icon size={14} /> {item.label}
                <span>{count || 0}</span>
              </button>
            )
          })}
        </div>

        {tab === 'attempts' && (
          data?.attempts.length ? (
            <div className="table-wrap">
              <table className="data-table">
                <thead><tr><th>Time</th><th>Strategy</th><th>Mode</th><th>Status</th><th className="num">Tip</th><th className="num">Fee</th><th className="num">Realized</th><th>Bundle</th><th /></tr></thead>
                <tbody>
                  {data.attempts.map((attempt) => (
                    <tr key={attempt.id} className="clickable" onClick={() => setSelectedAttempt(attempt)}>
                      <td><div className="cell-primary mono">{dateTime(attempt.ts_ms)}</div><div className="cell-secondary">{timeAgo(attempt.ts_ms)}</div></td>
                      <td><Badge tone="info">{attempt.strategy.replaceAll('_', ' ')}</Badge></td>
                      <td><Badge tone={attempt.mode === 'live' ? 'bad' : attempt.mode === 'canary' ? 'warn' : 'muted'}>{attempt.mode}</Badge></td>
                      <td><Badge tone={statusTone(attempt.status)} dot>{attempt.status.replaceAll('_', ' ')}</Badge></td>
                      <td className="num mono-dim">{lamports(attempt.tip_lamports)}</td>
                      <td className="num mono-dim">{lamports(attempt.fee_lamports)}</td>
                      <td className={cx('num mono', attempt.realized_net_lamports && attempt.realized_net_lamports < 0 ? 'text-bad' : 'text-good')}>
                        {attempt.realized_net_lamports === null ? '—' : signedSol(attempt.realized_net_lamports)}
                      </td>
                      <td className="mono-dim">{attempt.bundle_id ? shortAddress(attempt.bundle_id, 5, 4) : '—'}</td>
                      <td><ArrowUpRight size={14} className="row-arrow" /></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : <EmptyState icon={<Activity size={28} />} title="No attempts" description="In Canary, a simulated bundle appears here. Live attempts only happen after every gate is armed." />
        )}

        {tab === 'bundles' && (
          data?.bundles.length ? (
            <div className="table-wrap">
              <table className="data-table">
                <thead><tr><th>Submitted</th><th>Bundle</th><th>Status</th><th>Slot</th><th className="num">Tip</th><th>Signatures</th><th>Error</th></tr></thead>
                <tbody>
                  {data.bundles.map((bundle) => (
                    <tr key={bundle.bundle_id} className="clickable" onClick={() => setSelectedBundle(bundle)}>
                      <td><div className="cell-primary mono">{dateTime(bundle.ts_ms)}</div><div className="cell-secondary">{timeAgo(bundle.ts_ms)}</div></td>
                      <td><div className="cell-primary mono">{shortAddress(bundle.bundle_id, 8, 6)}</div><CopyButton value={bundle.bundle_id} /></td>
                      <td><Badge tone={statusTone(bundle.status)} dot>{bundle.status.replaceAll('_', ' ')}</Badge></td>
                      <td className="mono-dim">{bundle.landed_slot ?? '—'}</td>
                      <td className="num mono-dim">{lamports(bundle.tip_lamports)}</td>
                      <td className="mono-dim">{parseJson<string[]>(bundle.signatures_json, []).length} txs</td>
                      <td className="error-cell">{bundle.error || '—'}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : <EmptyState icon={<Boxes size={28} />} title="No bundles submitted" description="Bundles are only sent in Live mode after simulation and all risk gates pass." />
        )}

        {tab === 'settlements' && (
          settlements.length ? (
            <div className="table-wrap">
              <table className="data-table">
                <thead><tr><th>Settled</th><th>Opportunity</th><th>Signature</th><th>Slot</th><th className="num">Modeled</th><th className="num">Realized</th><th className="num">Tip</th><th className="num">Fee</th><th className="num">Model error</th></tr></thead>
                <tbody>
                  {settlements.map((row) => (
                    <tr key={row.opportunity_id}>
                      <td><div className="cell-primary mono">{dateTime(row.settled_at_ms)}</div><div className="cell-secondary">{timeAgo(row.settled_at_ms)}</div></td>
                      <td className="mono-dim">{shortAddress(row.opportunity_id, 6, 4)}</td>
                      <td><div className="cell-primary mono">{shortAddress(row.signature, 6, 6)}</div><CopyButton value={row.signature} /></td>
                      <td className="mono-dim">{row.slot}</td>
                      <td className="num mono-dim">{signedSol(row.modeled_net_lamports)}</td>
                      <td className={cx('num mono', row.realized_net_lamports >= 0 ? 'text-good' : 'text-bad')}>{signedSol(row.realized_net_lamports)}</td>
                      <td className="num mono-dim">{lamports(row.realized_tip_lamports)}</td>
                      <td className="num mono-dim">{lamports(row.realized_fee_lamports)}</td>
                      <td className="num"><Badge tone={modelErrorBps(row) > 500 ? 'bad' : modelErrorBps(row) > 150 ? 'warn' : 'good'}>{bps(modelErrorBps(row))}</Badge></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : <EmptyState icon={<Wallet size={28} />} title="No settlements" description="A settlement is written only after a landed bundle's pre/post wallet balances are reconciled." />
        )}

        {tab === 'events' && (
          data?.events.length ? (
            <div className="event-table">
              {data.events.map((event) => {
                let dataValue: unknown = null
                try { dataValue = JSON.parse(event.data_json) } catch { dataValue = event.data_json }
                return (
                  <details className="event-detail" key={event.id}>
                    <summary>
                      <span className={cx('dot', event.level === 'error' ? 'dot-bad' : event.level === 'warn' ? 'dot-warn' : 'dot-good')} />
                      <span className="event-time mono">{dateTime(event.ts_ms)}</span>
                      <Badge tone={event.level === 'error' ? 'bad' : event.level === 'warn' ? 'warn' : 'good'}>{event.level}</Badge>
                      <span className="event-component">{event.component}</span>
                      <strong>{event.message}</strong>
                      <span className="event-age">{timeAgo(event.ts_ms)}</span>
                    </summary>
                    <div className="json-box"><pre>{JSON.stringify(dataValue, null, 2)}</pre></div>
                  </details>
                )
              })}
            </div>
          ) : <EmptyState icon={<Terminal size={28} />} title="No events" description="Risk changes, execution outcomes and operator actions are appended to the event journal." />
        )}
      </Panel>

      <Modal open={selectedAttempt !== null} onClose={() => setSelectedAttempt(null)} title="Attempt detail" subtitle={selectedAttempt?.id}>
        {selectedAttempt && (
          <div className="detail-grid">
            <div className="detail-metrics">
              <div><span>Status</span><Badge tone={statusTone(selectedAttempt.status)}>{selectedAttempt.status.replaceAll('_', ' ')}</Badge></div>
              <div><span>Strategy</span><strong>{selectedAttempt.strategy.replaceAll('_', ' ')}</strong></div>
              <div><span>Mode</span><strong>{selectedAttempt.mode}</strong></div>
              <div><span>Tip</span><strong className="mono">{lamports(selectedAttempt.tip_lamports)}</strong></div>
              <div><span>Fee</span><strong className="mono">{lamports(selectedAttempt.fee_lamports)}</strong></div>
              <div><span>Realized</span><strong className={selectedAttempt.realized_net_lamports && selectedAttempt.realized_net_lamports < 0 ? 'text-bad' : 'text-good'}>{selectedAttempt.realized_net_lamports === null ? '—' : `${signedSol(selectedAttempt.realized_net_lamports)} SOL`}</strong></div>
            </div>
            <div className="detail-section">
              <h4>References</h4>
              <div className="ref-list">
                <span>Opportunity <code>{selectedAttempt.opportunity_id || '—'}</code></span>
                <span>Bundle <code>{selectedAttempt.bundle_id || '—'}</code></span>
                <span>Signature <code>{selectedAttempt.signature || '—'}</code></span>
              </div>
            </div>
            {selectedAttempt.error && <div className="detail-section"><h4>Error</h4><div className="form-error">{selectedAttempt.error}</div></div>}
            <div className="detail-foot"><span>Landed {String(selectedAttempt.landed)}</span><span>Reverted {String(selectedAttempt.reverted)}</span><span>Simulated {String(selectedAttempt.simulated)}</span></div>
          </div>
        )}
      </Modal>

      <Modal open={selectedBundle !== null} onClose={() => setSelectedBundle(null)} title="Bundle detail" subtitle={selectedBundle?.bundle_id} wide>
        {selectedBundle && (
          <div className="detail-grid">
            <div className="detail-metrics">
              <div><span>Status</span><Badge tone={statusTone(selectedBundle.status)}>{selectedBundle.status}</Badge></div>
              <div><span>Tip</span><strong className="mono">{lamports(selectedBundle.tip_lamports)}</strong></div>
              <div><span>Landed slot</span><strong>{selectedBundle.landed_slot ?? '—'}</strong></div>
              <div><span>Transactions</span><strong>{parseJson<string[]>(selectedBundle.signatures_json, []).length}</strong></div>
            </div>
            <div className="detail-section">
              <h4>Signatures</h4>
              <div className="ref-list">
                {parseJson<string[]>(selectedBundle.signatures_json, []).map((signature) => (
                  <span key={signature}><code>{signature}</code><CopyButton value={signature} /></span>
                ))}
                {!parseJson<string[]>(selectedBundle.signatures_json, []).length && <span>No signatures recorded.</span>}
              </div>
            </div>
            {selectedBundle.error && <div className="form-error">{selectedBundle.error}</div>}
            <div className="detail-foot"><span>Attempt {selectedBundle.attempt_id || '—'}</span><span>{dateTime(selectedBundle.ts_ms)}</span></div>
          </div>
        )}
      </Modal>
    </div>
  )
}

function modelErrorBps(row: Settlement): number {
  const denominator = Math.max(Math.abs(row.modeled_net_lamports), 1)
  return (Math.abs(row.realized_net_lamports - row.modeled_net_lamports) / denominator) * 10_000
}
