import { useState } from 'react'
import {
  AlertTriangle,
  Check,
  Coins,
  Copy,
  Recycle,
  RefreshCw,
  Search,
  ShieldCheck,
  Sparkles,
  Wallet,
} from 'lucide-react'
import { api } from '../api'
import type { Dashboard, RentReport, RentScan } from '../types'
import { Badge, Button, EmptyState, Modal, Panel, PanelHeader, StatCard } from '../components/ui'
import { cx, lamports, shortAddress, sol, timeAgo } from '../utils'

export function RentPage({
  data,
  token,
  notify,
  onRefresh,
}: {
  data: Dashboard | null
  token: string
  notify: (message: string, tone?: 'good' | 'bad' | 'info') => void
  onRefresh: () => void
}) {
  const [scan, setScan] = useState<RentScan | null>(null)
  const [report, setReport] = useState<RentReport | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState<'scan' | 'plan' | 'send' | null>(null)
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [confirmChecked, setConfirmChecked] = useState(false)
  const status = data?.status

  const doScan = async () => {
    setBusy('scan')
    setError(null)
    try {
      const result = await api.rent(token)
      setScan(result)
      notify(`Scanned ${result.accounts.length} token accounts`, 'good')
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'rent scan failed')
    } finally {
      setBusy(null)
    }
  }

  const doPlan = async () => {
    setBusy('plan')
    setError(null)
    try {
      const result = await api.reclaimRent(token, false)
      setReport(result)
      notify('Dry-run plan prepared; no transaction was sent', 'info')
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'rent plan failed')
    } finally {
      setBusy(null)
    }
  }

  const doSend = async () => {
    setBusy('send')
    setError(null)
    try {
      const result = await api.reclaimRent(token, true)
      setReport(result)
      setConfirmOpen(false)
      setConfirmChecked(false)
      notify(
        result.closed
          ? `Closed ${result.closed} accounts · recovered ${sol(result.net_recovered_lamports, 5)} SOL`
          : 'No accounts were closed',
        result.closed ? 'good' : 'info',
      )
      await doScan()
      onRefresh()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'rent reclaim failed')
    } finally {
      setBusy(null)
    }
  }

  const accounts = scan?.accounts || []
  const emptyAccounts = accounts.filter((account) => account.closable)
  const blockedAccounts = accounts.filter((account) => !account.closable)

  return (
    <div className="page-grid">
      <section className="rent-hero">
        <div>
          <div className="eyebrow"><Recycle size={13} /> ZERO-CAPITAL RECOVERY</div>
          <h2>Turn dead token accounts back into gas.</h2>
          <p>
            Closing an empty SPL Token account returns its rent-exempt lamports to your wallet. This engine
            never burns non-empty dust: it only closes accounts with an exact zero balance after verifying
            the on-chain owner and token program.
          </p>
          <div className="hero-actions">
            <Button variant="primary" onClick={doScan} loading={busy === 'scan'}><Search size={14} /> Scan wallet</Button>
            <Button variant="outline" onClick={doPlan} loading={busy === 'plan'} disabled={!scan}><Sparkles size={14} /> Dry-run plan</Button>
            <Button
              variant="danger"
              onClick={() => { setConfirmChecked(false); setConfirmOpen(true) }}
              disabled={!scan?.closable || !status?.signing_enabled}
              title={!status?.signing_enabled ? 'Live signing is not enabled' : undefined}
            >
              <Recycle size={14} /> Reclaim {scan?.closable ? `${scan.closable} accounts` : ''}
            </Button>
          </div>
        </div>
        <div className="rent-hero-stat">
          <span>RECOVERABLE NOW</span>
          <strong>{scan ? sol(scan.total_closable_lamports, 5) : '—'}</strong>
          <em>SOL</em>
          <div className="rent-hero-meta">
            <span>{scan ? `${scan.token_program_accounts} Token` : 'not scanned'}</span>
            <span>{scan ? `${scan.token_2022_accounts} Token-2022` : '—'}</span>
          </div>
        </div>
      </section>

      {!status?.rent_configured && (
        <div className="banner banner-warn">
          <AlertTriangle size={17} />
          <div>
            <strong>Rent reclaimer unavailable.</strong> Configure a wallet keypair and enable <code>[rent]</code> in
            config.toml, then restart the engine.
          </div>
        </div>
      )}
      {status && !status.signing_enabled && (
        <div className="banner banner-info">
          <ShieldCheck size={17} />
          <div>
            <strong>Read-only recovery posture.</strong> Scanning and dry-runs work now. Sending requires live mode
            (<code>--real</code> and <code>runtime.real=true</code>) and an explicit confirmation.
          </div>
        </div>
      )}
      {error && <div className="form-error">{error}</div>}

      <div className="stats-grid stats-grid-4">
        <StatCard label="Token accounts" value={scan ? String(accounts.length) : '—'} detail="Token + Token-2022 owned by wallet" icon={<Wallet size={18} />} tone="info" mono />
        <StatCard label="Closable" value={scan ? String(emptyAccounts.length) : '—'} detail="Exact zero balance, initialized" icon={<Recycle size={18} />} tone="good" mono />
        <StatCard label="Recoverable" value={scan ? `${sol(scan.total_closable_lamports, 5)} SOL` : '—'} detail="Gross rent, before network fees" icon={<Coins size={18} />} tone="good" mono />
        <StatCard label="Blocked" value={scan ? String(blockedAccounts.length) : '—'} detail="Non-empty, frozen or malformed" icon={<AlertTriangle size={18} />} tone={blockedAccounts.length ? 'warn' : 'muted'} mono />
      </div>

      <Panel>
        <PanelHeader
          title="Token accounts"
          subtitle={scan ? `Wallet ${shortAddress(scan.wallet, 6, 6)} · scanned ${timeAgo(scan.scanned_at_ms)}` : 'Scan the configured wallet to enumerate accounts'}
          icon={<Wallet size={17} />}
          actions={<Button variant="ghost" size="sm" onClick={doScan} loading={busy === 'scan'}><RefreshCw size={13} /> Rescan</Button>}
        />
        {scan ? (
          accounts.length ? (
            <div className="table-wrap">
              <table className="data-table">
                <thead><tr><th>Token account</th><th>Program</th><th>Mint</th><th className="num">Amount</th><th className="num">Lamports</th><th>Status</th><th>Reason</th></tr></thead>
                <tbody>
                  {accounts.map((account) => (
                    <tr key={account.address}>
                      <td><span className="mono">{shortAddress(account.address, 6, 6)}</span><CopyButtonInline value={account.address} /></td>
                      <td><Badge tone={account.token_program.includes('Tokenz') ? 'warn' : 'info'}>{account.token_program.includes('Tokenz') ? 'Token-2022' : 'Token'}</Badge></td>
                      <td className="mono-dim">{shortAddress(account.mint, 5, 5)}</td>
                      <td className={cx('num mono', account.amount > 0 ? 'text-warn' : 'text-good')}>{lamports(account.amount)}</td>
                      <td className="num mono">{lamports(account.lamports)}</td>
                      <td><Badge tone={account.closable ? 'good' : 'muted'} dot>{account.closable ? 'closable' : 'blocked'}</Badge></td>
                      <td className="reason-cell">{account.reason}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : (
            <EmptyState icon={<Check size={28} />} title="No token accounts found" description="This wallet has no SPL Token or Token-2022 accounts. Nothing to reclaim." />
          )
        ) : (
          <EmptyState icon={<Recycle size={28} />} title="No scan yet" description="Scan the configured wallet using getTokenAccountsByOwner. Nothing is signed during a scan." action={<Button variant="secondary" size="sm" onClick={doScan} loading={busy === 'scan'}>Scan wallet</Button>} />
        )}
      </Panel>

      {report && (
        <Panel>
          <PanelHeader
            title={report.dry_run ? 'Dry-run plan' : 'Reclaim report'}
            subtitle={`${report.attempted} accounts attempted · ${report.closed} closed · ${report.batches.length} batches`}
            icon={<Sparkles size={17} />}
            actions={<Badge tone={report.dry_run ? 'info' : report.errors.length ? 'warn' : 'good'}>{report.dry_run ? 'no transaction sent' : `${sol(report.net_recovered_lamports, 5)} SOL net`}</Badge>}
          />
          <div className="reclaim-summary">
            <div><span>Gross recovered</span><strong>{lamports(report.gross_recovered_lamports)}</strong></div>
            <div><span>Fees</span><strong>{lamports(report.fee_lamports)}</strong></div>
            <div><span>Net</span><strong className="text-good">{lamports(report.net_recovered_lamports)}</strong></div>
            <div><span>Closed</span><strong>{report.closed}</strong></div>
          </div>
          {report.batches.length > 0 && (
            <div className="batch-list">
              {report.batches.map((batch, index) => (
                <div className="batch-row" key={`${batch.accounts[0]}-${index}`}>
                  <span className="batch-index">{String(index + 1).padStart(2, '0')}</span>
                  <div><strong>{batch.accounts.length} accounts</strong><span>{lamports(batch.recovered_lamports)} lamports</span></div>
                  <Badge tone={batch.error ? 'bad' : batch.signature ? 'good' : 'info'}>
                    {batch.error ? 'failed' : batch.signature ? 'sent' : 'planned'}
                  </Badge>
                  <span className="mono-dim">{batch.signature ? shortAddress(batch.signature, 6, 5) : '—'}</span>
                </div>
              ))}
            </div>
          )}
          {report.errors.length > 0 && (
            <div className="error-list">
              {report.errors.map((item, index) => <div key={index}>{item}</div>)}
            </div>
          )}
        </Panel>
      )}

      <div className="grid-1-1">
        <Panel>
          <PanelHeader title="Safety model" subtitle="Rent recovery is deliberately boring" icon={<ShieldCheck size={17} />} />
          <div className="safety-list">
            <div><Check size={14} /><span>Only accounts whose decoded amount is exactly zero.</span></div>
            <div><Check size={14} /><span>Owner and token program are verified against the wallet and registry.</span></div>
            <div><Check size={14} /><span>Every batch is simulated before a signature is sent.</span></div>
            <div><Check size={14} /><span>Non-empty dust is never burned; users keep custody.</span></div>
            <div><Check size={14} /><span>Kill switch blocks all rent transactions.</span></div>
          </div>
        </Panel>
        <Panel>
          <PanelHeader title="Why recover rent?" subtitle="Solana charges rent-exempt deposits per token account" icon={<Coins size={17} />} />
          <div className="rent-explain">
            <p>Every SPL Token account holds a refundable rent-exempt balance. When a token position is fully exited, that account can be closed and its lamports returned.</p>
            <p>This is zero-capital, non-speculative income. It does not need a trade, a bundle, or a price edge — only your own empty accounts.</p>
            <div className="rent-explain-stat"><span>Typical rent per token account</span><strong>0.00203928 SOL</strong></div>
          </div>
        </Panel>
      </div>

      <Modal
        open={confirmOpen}
        onClose={() => setConfirmOpen(false)}
        title="Send rent-close transactions"
        subtitle={`${scan?.closable || 0} accounts · ${scan ? sol(scan.total_closable_lamports, 5) : '0'} SOL gross`}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmOpen(false)}>Cancel</Button>
            <Button variant="danger" disabled={!confirmChecked} loading={busy === 'send'} onClick={doSend}>
              <Recycle size={14} /> Sign & send
            </Button>
          </>
        }
      >
        <div className="confirm-body">
          <div className="confirm-warning"><AlertTriangle size={18} /><span>This signs with the configured hot wallet and sends real transactions. Only zero-balance accounts are selected.</span></div>
          <label className="check-row">
            <input type="checkbox" checked={confirmChecked} onChange={(event) => setConfirmChecked(event.target.checked)} />
            <span>I understand these are real Solana transactions and I want to close {scan?.closable || 0} empty token accounts.</span>
          </label>
          <div className="confirm-list">
            {emptyAccounts.slice(0, 8).map((account) => (
              <div key={account.address}><span className="mono">{shortAddress(account.address, 6, 6)}</span><span>{lamports(account.lamports)}</span></div>
            ))}
            {emptyAccounts.length > 8 && <div className="mono-dim">…and {emptyAccounts.length - 8} more</div>}
          </div>
          {!status?.signing_enabled && <div className="form-error">Live signing is disabled; the API will reject this request. Enable --real and runtime.real first.</div>}
        </div>
      </Modal>
    </div>
  )
}

function CopyButtonInline({ value }: { value: string }) {
  return (
    <button
      className="inline-copy"
      onClick={() => void navigator.clipboard?.writeText(value)}
      title="Copy account"
    >
      <Copy size={11} />
    </button>
  )
}
