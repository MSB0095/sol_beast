import { useState } from 'react'
import {
  Activity,
  ArrowRight,
  Crosshair,
  Gauge,
  Layers,
  Recycle,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  Target,
  Zap,
} from 'lucide-react'
import { api } from '../api'
import type { Dashboard, Mode, StrategyState } from '../types'
import type { PageId } from '../components/Shell'
import { Badge, EmptyState, Panel, PanelHeader } from '../components/ui'
import { cx, modeLabel, modeRank } from '../utils'

const STRATEGY_META: Record<string, { title: string; description: string; icon: typeof Activity; kind: 'arb' | 'rent' }> = {
  cross_dex_arb: {
    title: 'Cross-DEX arbitrage',
    description: 'Same-mint dislocation across two venues, closed atomically in one bundle.',
    icon: Crosshair,
    kind: 'arb',
  },
  triangular_arb: {
    title: 'Triangular arbitrage',
    description: 'Three-leg cycle returning to the start mint through independent pools.',
    icon: Layers,
    kind: 'arb',
  },
  lst_arb: {
    title: 'LST arbitrage',
    description: 'Liquid staking token redemption and secondary-market spread.',
    icon: Sparkles,
    kind: 'arb',
  },
  stable_arb: {
    title: 'Stablecoin arbitrage',
    description: 'Stable and correlated-asset pools with low-impact curves.',
    icon: Target,
    kind: 'arb',
  },
  flash_arb: {
    title: 'Flash-loan arbitrage',
    description: 'Borrow from a Kamino reserve inside the cycle; repay principal + fee before the transaction can land.',
    icon: Zap,
    kind: 'arb',
  },
  rent_reclaim: {
    title: 'Rent reclaimer',
    description: 'Close the wallet’s own empty token accounts and recover rent-exempt SOL.',
    icon: Recycle,
    kind: 'rent',
  },
}

const MODES: Mode[] = ['off', 'radar', 'canary', 'live']

export function StrategiesPage({
  data,
  token,
  notify,
  onRefresh,
  onNavigate,
}: {
  data: Dashboard | null
  token: string
  notify: (message: string, tone?: 'good' | 'bad' | 'info') => void
  onRefresh: () => void
  onNavigate: (page: PageId) => void
}) {
  const [busy, setBusy] = useState<string | null>(null)
  const processMode = data?.status.mode || 'off'
  const ceiling = data?.status.configured_mode || 'radar'

  const change = async (strategy: StrategyState, mode: Mode) => {
    setBusy(strategy.name)
    try {
      await api.setStrategy(token, strategy.name, mode)
      notify(`${strategy.name.replaceAll('_', ' ')} set to ${mode}`, 'good')
      onRefresh()
    } catch (error) {
      notify(error instanceof Error ? error.message : 'mode change failed', 'bad')
    } finally {
      setBusy(null)
    }
  }

  return (
    <div className="page-grid">
      <section className="strategy-hero">
        <div>
          <div className="eyebrow"><ShieldCheck size={13} /> INDEPENDENT ARMS</div>
          <h2>One engine. Five arb arms. One rent reclaimer.</h2>
          <p>
            Every arm has its own mode ceiling, cost budget, impact cap and guard requirement. The
            process mode is a hard upper bound: a strategy can be quieter than the engine, never louder.
          </p>
        </div>
        <div className="mode-ladder">
          {MODES.map((mode, index) => {
            const active = modeRank(processMode) >= modeRank(mode)
            const allowed = modeRank(mode) <= modeRank(ceiling)
            return (
              <div className={cx('ladder-step', active && 'ladder-active', !allowed && 'ladder-disabled')} key={mode}>
                <span>{String(index + 1).padStart(2, '0')}</span>
                <strong>{modeLabel(mode)}</strong>
                <em>{mode === 'off' ? 'halted' : mode === 'radar' ? 'detect only' : mode === 'canary' ? 'simulate bundles' : 'sign + guard'}</em>
              </div>
            )
          })}
        </div>
      </section>

      <div className="banner banner-info">
        <Gauge size={17} />
        <div>
          <strong>Process ceiling: {modeLabel(ceiling)}.</strong> Strategy requests above this ceiling are rejected by the API. Current process mode is <strong>{modeLabel(processMode)}</strong>; promotion is deliberate, never automatic.
        </div>
      </div>

      <Panel>
        <PanelHeader
          title="Flash provider"
          subtitle="Resolved from live on-chain reserve state at startup"
          icon={<Zap size={17} />}
        />
        {data?.status.flash ? (
          <div className="gate-list">
            <div>
              <span>
                {data.status.flash.provider.toUpperCase()}
                {data.status.flash.mint
                  ? ` · ${data.status.flash.mint.slice(0, 4)}…${data.status.flash.mint.slice(-4)}`
                  : ' · no reserve'}
              </span>
              <strong>
                {data.status.flash.fee_ppm !== null
                  ? `${(data.status.flash.fee_ppm / 1_000_000).toFixed(6)}% fee`
                  : 'no active reserve'}
              </strong>
              <p>
                {data.status.flash.reserve && data.status.flash.available !== null && data.status.flash.decimals !== null
                  ? `Reserve ${data.status.flash.reserve.slice(0, 8)}…${data.status.flash.reserve.slice(-8)} · ${(data.status.flash.available / 10 ** data.status.flash.decimals).toLocaleString(undefined, { maximumFractionDigits: 2 })} available · `
                  : ''}
                borrow ceiling {(data.status.flash.max_borrow_lamports / 1e9).toFixed(3)} SOL-equivalent ·
                {' '}{data.status.flash.reserve_count} borrowable {data.status.flash.reserve_count === 1 ? 'mint' : 'mints'} loaded.
              </p>
            </div>
            <div>
              <span>REPAYMENT</span>
              <strong>atomic</strong>
              <p>
                {data.status.flash.provider === 'marginfi'
                  ? 'Principal and any origination fee are debited inside the flashloan flag, which skips the health check. A cycle that cannot repay reverts.'
                  : 'Principal and the reserve fee are debited in the same instruction. A cycle that cannot repay reverts.'}{' '}
                The separate lamport guard is {data.status.flash.lamport_guard ? 'used for WSOL cycles and omitted for non-WSOL cycles' : 'omitted because no WSOL reserve is loaded'}.
              </p>
            </div>
          </div>
        ) : (
          <EmptyState
            icon={<Zap size={24} />}
            title="Flash borrowing is disabled"
            description="Set [flash] enabled=true and arm flash_arb to borrow inside a cycle. Disabled is the default."
          />
        )}
      </Panel>

      {data?.strategies.length ? (
        <div className="strategy-grid">
          {data.strategies.map((strategy) => {
            const meta = STRATEGY_META[strategy.name]
            const Icon = meta?.icon || Activity
            const title = meta?.title || strategy.name.replaceAll('_', ' ')
            const effective = modeRank(strategy.mode) > modeRank(processMode) ? processMode : strategy.mode
            const disabled = busy === strategy.name
            return (
              <Panel key={strategy.name} className={cx('strategy-card', effective !== 'off' && 'strategy-card-armed')}>
                <div className="strategy-card-head">
                  <span className={cx('strategy-icon', effective !== 'off' && 'strategy-icon-armed')}><Icon size={19} /></span>
                  <div className="strategy-card-title">
                    <h3>{title}</h3>
                    <span>{strategy.name}</span>
                  </div>
                  <Badge tone={effective === 'live' ? 'bad' : effective === 'canary' ? 'warn' : effective === 'radar' ? 'good' : 'muted'} dot>
                    {modeLabel(effective)}
                  </Badge>
                </div>
                <p>{meta?.description || 'Registered strategy.'}</p>
                <div className="strategy-tags">
                  {strategy.requires_bundle_guard ? <Badge tone="warn">bundle + guard</Badge> : <Badge tone="good">owner-only</Badge>}
                  <Badge tone="good">wired</Badge>
                  <Badge tone={strategy.configured ? 'info' : 'muted'}>{strategy.configured ? 'configured' : 'using defaults'}</Badge>
                </div>
                <div className="mode-buttons">
                  {MODES.map((mode) => {
                    const allowed = modeRank(mode) <= modeRank(ceiling)
                    return (
                      <button
                        key={mode}
                        className={cx('mode-button', strategy.mode === mode && 'mode-button-active', mode === 'live' && 'mode-button-live')}
                        disabled={disabled || !allowed}
                        onClick={() => void change(strategy, mode)}
                        title={!allowed ? `Exceeds configured ceiling ${ceiling}` : undefined}
                      >
                        {mode}
                      </button>
                    )
                  })}
                </div>
                {strategy.name === 'rent_reclaim' && (
                  <button className="strategy-link" onClick={() => onNavigate('rent')}>Open rent reclaimer <ArrowRight size={13} /></button>
                )}
              </Panel>
            )
          })}
        </div>
      ) : (
        <Panel><EmptyState icon={<Activity size={28} />} title="No strategies loaded" description="Connect to the API to inspect strategy arms." /></Panel>
      )}

      <div className="grid-1-1">
        <Panel>
          <PanelHeader title="Promotion gates" subtitle="The roadmap’s evidence thresholds" icon={<ShieldCheck size={17} />} />
          <div className="gate-list">
            <div><span>RADAR → CANARY</span><strong>14 days</strong><p>At least one simulated or dropped attempt with no unexplained deltas.</p></div>
            <div><span>CANARY → LIVE</span><strong>14 days</strong><p>Stable simulations, acceptable model error, guard deployed and fork-tested.</p></div>
            <div><span>LIVE</span><strong>per trade</strong><p>On-chain guard must prove net ≥ min profit after tip, fees, rent and repayment.</p></div>
          </div>
        </Panel>
        <Panel>
          <PanelHeader title="Strategy safety" subtitle="What the engine refuses to do" icon={<ShieldAlert size={17} />} />
          <div className="safety-list">
            <div><ShieldCheck size={14} /><span>No sandwiching, front-running, or victim transaction ordering.</span></div>
            <div><ShieldCheck size={14} /><span>Flash cycles only use a provider and reserve proven by a live mainnet simulation. The borrow, swaps and repayment share one transaction, so an unprofitable cycle reverts on-chain.</span></div>
            <div><ShieldCheck size={14} /><span>Non-WSOL flash cycles keep profit in the borrowed token and therefore run without the separate lamport guard; their protection is the enforced final min-out plus the flash repayment.</span></div>
            <div><ShieldCheck size={14} /><span>No live attempt without a positive guard threshold and deployed program, unless unguarded live mode is explicitly enabled.</span></div>
            <div><ShieldCheck size={14} /><span>No hidden model assumptions: realized PnL is measured from pre/post balances.</span></div>
            <div><ShieldCheck size={14} /><span>Kill switch halts every signing path immediately.</span></div>
          </div>
        </Panel>
      </div>
    </div>
  )
}
