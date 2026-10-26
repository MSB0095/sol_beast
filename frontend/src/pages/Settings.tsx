import { useCallback, useEffect, useState } from 'react'
import {
  AlertTriangle,
  ArrowDownToLine,
  CheckCircle2,
  Clipboard,
  Clock3,
  Code2,
  Cpu,
  Database,
  FileText,
  Fingerprint,
  HeartPulse,
  KeyRound,
  LockKeyhole,
  RefreshCw,
  Server,
  ShieldAlert,
  ShieldCheck,
  Skull,
  Terminal,
  Wifi,
} from 'lucide-react'
import { api } from '../api'
import type { Dashboard } from '../types'
import { Badge, Button, EmptyState, KeyValue, Modal, Panel, PanelHeader, Spinner, TextArea } from '../components/ui'
import { cx, dateTime, duration, modeLabel, timeAgo } from '../utils'

type Tab = 'overview' | 'config' | 'health' | 'actions'

export function SettingsPage({
  data,
  token,
  lastUpdated,
  notify,
  onRefresh,
  onLogout,
}: {
  data: Dashboard | null
  token: string
  lastUpdated: number | null
  notify: (message: string, tone?: 'good' | 'bad' | 'info') => void
  onRefresh: () => void
  onLogout: () => void
}) {
  const [tab, setTab] = useState<Tab>('overview')
  const [config, setConfig] = useState<Record<string, unknown> | null>(null)
  const [configError, setConfigError] = useState<string | null>(null)
  const [configLoading, setConfigLoading] = useState(false)
  const [killOpen, setKillOpen] = useState(false)
  const [killReason, setKillReason] = useState('manual halt from dashboard')
  const [busy, setBusy] = useState<string | null>(null)
  const [health, setHealth] = useState<{ status: string; version: string; uptime_seconds: number } | null>(null)
  const [healthError, setHealthError] = useState<string | null>(null)
  const status = data?.status

  const loadConfig = useCallback(async () => {
    setConfigLoading(true)
    setConfigError(null)
    try {
      setConfig(await api.config(token))
    } catch (error) {
      setConfigError(error instanceof Error ? error.message : 'cannot load configuration')
    } finally {
      setConfigLoading(false)
    }
  }, [token])

  useEffect(() => {
    if (tab === 'config' && !config && !configLoading) void loadConfig()
  }, [tab, config, configLoading, loadConfig])

  const kill = async () => {
    setBusy('kill')
    try {
      await api.kill(token, killReason)
      notify('Kill switch activated; all signing halted', 'bad')
      setKillOpen(false)
      onRefresh()
    } catch (error) {
      notify(error instanceof Error ? error.message : 'kill failed', 'bad')
    } finally {
      setBusy(null)
    }
  }

  const resume = async () => {
    setBusy('resume')
    try {
      await api.resume(token)
      notify('Kill switch cleared', 'good')
      onRefresh()
    } catch (error) {
      notify(error instanceof Error ? error.message : 'resume failed', 'bad')
    } finally {
      setBusy(null)
    }
  }

  const downloadReport = async () => {
    setBusy('report')
    try {
      const report = await api.report(token)
      const blob = new Blob([JSON.stringify(report, null, 2)], { type: 'application/json' })
      const url = URL.createObjectURL(blob)
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = `sol-beast-report-${new Date().toISOString().slice(0, 19).replaceAll(':', '-')}.json`
      anchor.click()
      URL.revokeObjectURL(url)
      notify('Report downloaded', 'good')
    } catch (error) {
      notify(error instanceof Error ? error.message : 'report failed', 'bad')
    } finally {
      setBusy(null)
    }
  }

  const checkHealth = async () => {
    setBusy('health')
    setHealthError(null)
    try {
      setHealth(await api.health())
      notify('API health check passed', 'good')
    } catch (error) {
      setHealthError(error instanceof Error ? error.message : 'health check failed')
    } finally {
      setBusy(null)
    }
  }

  const configText = config ? JSON.stringify(config, null, 2) : ''
  const liveReady =
    status?.signing_enabled &&
    status.jito_enabled &&
    status.guard.enabled &&
    status.guard.mode === 'program' &&
    status.guard.program_id_configured &&
    status.wallet.keypair_configured

  return (
    <div className="page-grid">
      <div className="settings-tabs">
        {([
          ['overview', 'System overview', Server],
          ['config', 'Configuration', Code2],
          ['health', 'Health checks', HeartPulse],
          ['actions', 'Operator actions', Skull],
        ] as Array<[Tab, string, typeof Server]>).map(([id, label, Icon]) => (
          <button key={id} className={cx('settings-tab', tab === id && 'settings-tab-active')} onClick={() => setTab(id)}>
            <Icon size={15} /> {label}
          </button>
        ))}
      </div>

      {tab === 'overview' && (
        <>
          <div className="grid-1-1">
            <Panel>
              <PanelHeader title="Engine identity" subtitle="Local process and build" icon={<Cpu size={17} />} actions={<Badge tone={liveReady ? 'good' : 'warn'}>{liveReady ? 'live ready' : 'gated'}</Badge>} />
              <div className="kv-list">
                <KeyValue label="Service">{status?.service || '—'}</KeyValue>
                <KeyValue label="Version"><span className="mono">{status?.version || '—'}</span></KeyValue>
                <KeyValue label="Uptime">{duration(status?.uptime_seconds)}</KeyValue>
                <KeyValue label="Process mode"><Badge tone={status?.mode === 'live' ? 'bad' : status?.mode === 'canary' ? 'warn' : 'good'}>{modeLabel(status?.mode || 'off')}</Badge></KeyValue>
                <KeyValue label="Configured ceiling">{modeLabel(status?.configured_mode || 'radar')}</KeyValue>
                <KeyValue label="Signing posture">{status?.signing_enabled ? <span className="text-warn">armed</span> : 'radar only'}</KeyValue>
                <KeyValue label="Kill switch">{status?.kill_switch ? <Badge tone="bad">active</Badge> : <Badge tone="good">clear</Badge>}</KeyValue>
              </div>
            </Panel>
            <Panel>
              <PanelHeader title="Control plane" subtitle="Loopback API and auth" icon={<Fingerprint size={17} />} />
              <div className="kv-list">
                <KeyValue label="API bind"><span className="mono">{status?.api.bind || '—'}</span></KeyValue>
                <KeyValue label="API enabled">{status?.api.enabled ? <Badge tone="good">yes</Badge> : <Badge tone="bad">no</Badge>}</KeyValue>
                <KeyValue label="Session token"><span className="mono">••••••••{token.slice(-6)}</span></KeyValue>
                <KeyValue label="CORS policy">explicit allowlist</KeyValue>
                <KeyValue label="Secrets">secrets.toml / 0600</KeyValue>
                <KeyValue label="Read/write split">distinct bearer tokens</KeyValue>
              </div>
              <div className="settings-inline-actions">
                <Button variant="secondary" size="sm" onClick={onLogout}><LockKeyhole size={13} /> Lock session</Button>
                <Button variant="ghost" size="sm" onClick={() => void navigator.clipboard?.writeText(status?.api.bind || '')}><Clipboard size={13} /> Copy bind</Button>
              </div>
            </Panel>
          </div>

          <div className="grid-1-1">
            <Panel>
              <PanelHeader title="Execution gates" subtitle="Required before a live signature" icon={<ShieldCheck size={17} />} />
              <div className="settings-gates">
                <GateRow label="--real + runtime.real" value={status?.signing_enabled || false} />
                <GateRow label="Jito bundles" value={status?.jito_enabled || false} />
                <GateRow label="Guard enabled" value={status?.guard.enabled || false} />
                <GateRow label="Guard program" value={status?.guard.program_id_configured || false} />
                <GateRow label="Wallet keypair" value={status?.wallet.keypair_configured || false} />
                <GateRow label="Kill switch clear" value={!status?.kill_switch} />
              </div>
            </Panel>
            <Panel>
              <PanelHeader title="Store & telemetry" subtitle="SQLite journal and counters" icon={<Database size={17} />} />
              <div className="store-grid">
                <div><span>Pools</span><strong>{status?.store.pools ?? 0}</strong></div>
                <div><span>Opportunities</span><strong>{status?.store.opportunities ?? 0}</strong></div>
                <div><span>Attempts</span><strong>{status?.store.attempts ?? 0}</strong></div>
                <div><span>Bundles</span><strong>{status?.store.bundles ?? 0}</strong></div>
                <div><span>Settlements</span><strong>{status?.store.settlements ?? 0}</strong></div>
                <div><span>Events</span><strong>{status?.store.events ?? 0}</strong></div>
              </div>
              <div className="settings-inline-actions">
                <Button variant="secondary" size="sm" loading={busy === 'report'} onClick={downloadReport}><ArrowDownToLine size={13} /> Download report</Button>
                <Button variant="ghost" size="sm" onClick={onRefresh}><RefreshCw size={13} /> Refresh telemetry</Button>
              </div>
            </Panel>
          </div>

          <Panel>
            <PanelHeader title="Recent operator events" subtitle="Audit trail from the store" icon={<Clock3 size={17} />} />
            <div className="settings-events">
              {data?.events.slice(0, 8).map((event) => (
                <div key={event.id}>
                  <span className={cx('dot', event.level === 'error' ? 'dot-bad' : event.level === 'warn' ? 'dot-warn' : 'dot-good')} />
                  <span className="mono">{dateTime(event.ts_ms)}</span>
                  <Badge tone={event.level === 'error' ? 'bad' : event.level === 'warn' ? 'warn' : 'good'}>{event.level}</Badge>
                  <span>{event.component}</span>
                  <strong>{event.message}</strong>
                </div>
              ))}
              {!data?.events.length && <EmptyState compact icon={<FileText size={22} />} title="No events" description="The event journal is empty." />}
            </div>
          </Panel>
        </>
      )}

      {tab === 'config' && (
        <Panel>
          <PanelHeader
            title="Redacted configuration"
            subtitle="RPC credentials, Jupiter keys and Jito URLs are sanitized by the API"
            icon={<Code2 size={17} />}
            actions={
              <>
                <Button variant="ghost" size="sm" onClick={loadConfig} loading={configLoading}><RefreshCw size={13} /> Reload</Button>
                <Button variant="secondary" size="sm" disabled={!configText} onClick={() => void navigator.clipboard?.writeText(configText)}><Clipboard size={13} /> Copy JSON</Button>
              </>
            }
          />
          {configLoading ? <Spinner label="Loading redacted config" /> : configError ? <div className="form-error">{configError}</div> : config ? (
            <div className="config-view">
              <div className="config-note"><ShieldCheck size={14} /> Secrets are never returned. Edit config.toml and restart to change them.</div>
              <pre>{configText}</pre>
            </div>
          ) : <EmptyState icon={<Code2 size={26} />} title="Configuration unavailable" description="The API did not return a config snapshot." />}
        </Panel>
      )}

      {tab === 'health' && (
        <div className="grid-1-1">
          <Panel>
            <PanelHeader title="Live probes" subtitle="Run a direct API health check" icon={<HeartPulse size={17} />} actions={<Button variant="secondary" size="sm" onClick={checkHealth} loading={busy === 'health'}><Wifi size={13} /> Run probe</Button>} />
            <div className="health-list">
              <HealthRow label="API process" ok={Boolean(status)} detail={status ? `up ${duration(status.uptime_seconds)}` : 'no telemetry'} />
              <HealthRow label="Store" ok={Boolean(status?.store)} detail={status ? `${status.store.opportunities} opportunities persisted` : '—'} />
              <HealthRow label="Feed" ok={Boolean(status?.feed && status.feed.stale === 0)} detail={status?.feed ? `${status.feed.quotable}/${status.feed.pools} quotable · ${status.feed.stale} stale` : 'not configured'} />
              <HealthRow label="Jupiter" ok={Boolean(status?.jupiter_configured)} detail={status?.jupiter_configured ? 'quote client configured' : 'missing'} />
              <HealthRow label="Jito" ok={Boolean(status?.jito_enabled)} detail={status?.jito_enabled ? 'bundle client enabled' : 'disabled'} />
              <HealthRow label="Guard program" ok={Boolean(status?.guard.program_id_configured)} detail={status?.guard.program_id_configured ? status.guard.mode : 'unset'} />
              <HealthRow label="Wallet" ok={Boolean(status?.wallet.keypair_configured)} detail={status?.wallet.keypair_configured ? 'keypair present' : 'missing'} />
            </div>
          </Panel>
          <Panel>
            <PanelHeader title="Probe result" subtitle="Public /healthz response" icon={<Terminal size={17} />} />
            {healthError && <div className="form-error">{healthError}</div>}
            {health ? (
              <div className="json-box"><pre>{JSON.stringify(health, null, 2)}</pre></div>
            ) : (
              <EmptyState compact icon={<Terminal size={22} />} title="No probe run yet" description="Run a probe to verify the API process independently of the authenticated telemetry path." />
            )}
            <div className="kv-list health-kv">
              <KeyValue label="Last telemetry">{lastUpdated ? timeAgo(lastUpdated) : '—'}</KeyValue>
              <KeyValue label="Feed age">{status?.feed?.age_ms != null ? `${status.feed.age_ms} ms` : '—'}</KeyValue>
              <KeyValue label="Feed last error">{status?.feed?.last_error || 'none'}</KeyValue>
              <KeyValue label="Risk state">{status?.risk?.state || '—'}</KeyValue>
            </div>
          </Panel>
        </div>
      )}

      {tab === 'actions' && (
        <div className="grid-1-1">
          <Panel>
            <PanelHeader title="Emergency stop" subtitle="Halt signing and mark risk halted" icon={<ShieldAlert size={17} />} />
            <div className="action-card">
              <div className="action-icon danger"><Skull size={22} /></div>
              <div>
                <h3>Activate kill switch</h3>
                <p>Writes the kill-switch file, sets risk to halted, and blocks rent or live submissions. Mode changes cannot re-enable live while active.</p>
              </div>
            </div>
            <Button variant="danger" disabled={status?.kill_switch} onClick={() => setKillOpen(true)}><Skull size={14} /> Activate kill switch</Button>
            {status?.kill_switch && (
              <div className="resume-box">
                <Badge tone="bad">active</Badge>
                <span>Risk reasons: {status.risk?.reasons.map((reason) => reason.detail).join(' · ') || 'manual halt'}</span>
                <Button variant="secondary" size="sm" onClick={resume} loading={busy === 'resume'}><CheckCircle2 size={13} /> Resume</Button>
              </div>
            )}
          </Panel>
          <Panel>
            <PanelHeader title="Session & maintenance" subtitle="Non-destructive actions" icon={<KeyRound size={17} />} />
            <div className="action-list">
              <button onClick={downloadReport}><ArrowDownToLine size={16} /><div><strong>Download settlement report</strong><span>JSON report with strategy summary and on-chain settlements</span></div></button>
              <button onClick={onRefresh}><RefreshCw size={16} /><div><strong>Refresh telemetry</strong><span>Force a dashboard snapshot now</span></div></button>
              <button onClick={onLogout}><LockKeyhole size={16} /><div><strong>Lock session</strong><span>Remove the bearer token from this browser tab</span></div></button>
            </div>
            <div className="settings-note">
              <AlertTriangle size={14} />
              <span>Configuration is intentionally read-only in the dashboard. Edit <code>config.toml</code> under review, run <code>beast check</code>, then restart the process.</span>
            </div>
          </Panel>
        </div>
      )}

      <Modal
        open={killOpen}
        onClose={() => setKillOpen(false)}
        title="Activate kill switch"
        subtitle="All signing stops immediately"
        footer={
          <>
            <Button variant="ghost" onClick={() => setKillOpen(false)}>Cancel</Button>
            <Button variant="danger" loading={busy === 'kill'} onClick={kill}><Skull size={14} /> Halt everything</Button>
          </>
        }
      >
        <div className="confirm-body">
          <div className="confirm-warning"><AlertTriangle size={18} /><span>The engine will write a kill-switch file and refuse live or rent transactions until you explicitly resume.</span></div>
          <TextArea label="Reason (journaled)" value={killReason} onChange={(event) => setKillReason(event.target.value)} />
        </div>
      </Modal>
    </div>
  )
}

function GateRow({ label, value }: { label: string; value: boolean }) {
  return (
    <div className="gate-row">
      <span className={cx('dot', value ? 'dot-good' : 'dot-warn')} />
      <span>{label}</span>
      <Badge tone={value ? 'good' : 'warn'}>{value ? 'ready' : 'gated'}</Badge>
    </div>
  )
}

function HealthRow({ label, ok, detail }: { label: string; ok: boolean; detail: string }) {
  return (
    <div className="health-row">
      <span className={cx('dot', ok ? 'dot-good' : 'dot-bad')} />
      <strong>{label}</strong>
      <span>{detail}</span>
    </div>
  )
}
