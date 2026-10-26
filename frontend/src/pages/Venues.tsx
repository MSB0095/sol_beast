import { useMemo, useState } from 'react'
import {
  ArrowUpRight,
  BookOpen,
  Boxes,
  Code2,
  Database,
  Filter,
  Globe2,
  Network,
  RefreshCw,
  Search,
  Sparkles,
} from 'lucide-react'
import type { Venue, VenuesResponse } from '../types'
import { Badge, Button, CopyButton, EmptyState, Modal, Panel, PanelHeader } from '../components/ui'
import { coverageLabel, coverageTone, cx, shortAddress } from '../utils'

const COVERAGE_HELP: Record<string, string> = {
  modeled: 'Verified local math: pool state is decoded and the swap curve is computed in-process.',
  decoded: 'Local account decoder with verified reserves; routing can use this venue without Jupiter.',
  quote_only: 'Routed through Jupiter’s aggregator; no unverified local instruction builder is used.',
  catalogued: 'Program is known and tracked for coverage, but needs an adapter before routing.',
  stake_pool: 'Liquid staking pool; used for LST pricing and redemption routes.',
  disabled: 'Intentionally disabled pending verification or because the program is deprecated.',
}

export function VenuesPage({
  data,
  onRefresh,
}: {
  data: VenuesResponse | null
  onRefresh: () => void
}) {
  const [search, setSearch] = useState('')
  const [coverage, setCoverage] = useState('all')
  const [kind, setKind] = useState('all')
  const [selected, setSelected] = useState<Venue | null>(null)

  const venues = useMemo(() => data?.venues || [], [data?.venues])
  const coverages = useMemo(() => Array.from(new Set(venues.map((venue) => venue.coverage))).sort(), [venues])
  const kinds = useMemo(() => Array.from(new Set(venues.map((venue) => venue.kind))).sort(), [venues])
  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase()
    return venues.filter((venue) => {
      if (coverage !== 'all' && venue.coverage !== coverage) return false
      if (kind !== 'all' && venue.kind !== kind) return false
      if (!needle) return true
      return (
        venue.name.toLowerCase().includes(needle) ||
        venue.id.toLowerCase().includes(needle) ||
        venue.program_id.toLowerCase().includes(needle) ||
        (venue.notes || '').toLowerCase().includes(needle)
      )
    })
  }, [coverage, kind, search, venues])

  return (
    <div className="page-grid">
      <section className="venue-hero">
        <div>
          <div className="eyebrow"><Globe2 size={13} /> SOLANA LIQUIDITY UNIVERSE</div>
          <h2>Every registered venue. One routing brain.</h2>
          <p>
            A static, auditable registry of Solana DEXes, AMMs, CLOBs, stableswap pools, LST pools and prop AMMs.
            Verified local models and decoders come first; quote-only venues route through Jupiter with explicit
            stale-data and cost checks. Catalogued and disabled venues are never routed.
          </p>
        </div>
        <div className="venue-count">
          <strong>{data?.count || 0}</strong>
          <span>REGISTERED PROGRAMS</span>
          <div>
            <span>{data?.coverage.modeled || 0} modeled</span>
            <span>{data?.coverage.quote_only || 0} quote-only</span>
          </div>
        </div>
      </section>

      <div className="coverage-cards">
        {Object.entries(data?.coverage || {})
          .sort((a, b) => b[1] - a[1])
          .map(([key, count]) => (
            <button
              key={key}
              className={cx('coverage-card', coverage === key && 'coverage-card-active')}
              onClick={() => setCoverage((value) => (value === key ? 'all' : key))}
            >
              <span className={cx('coverage-card-icon', `tone-${coverageTone(key)}`)}>
                {key === 'modeled' ? <Sparkles size={16} /> : key === 'quote_only' ? <Globe2 size={16} /> : key === 'decoded' ? <Code2 size={16} /> : <Database size={16} />}
              </span>
              <strong>{count}</strong>
              <span>{coverageLabel(key)}</span>
            </button>
          ))}
      </div>

      <Panel>
        <PanelHeader
          title="Venue registry"
          subtitle="Coverage is an explicit claim, not a marketing number"
          icon={<Network size={17} />}
          actions={<Button variant="ghost" size="sm" onClick={onRefresh}><RefreshCw size={13} /> Reload</Button>}
        />
        <div className="toolbar">
          <div className="search-wrap">
            <Search size={14} />
            <input className="search-input" value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search name, program, notes" />
          </div>
          <select className="input input-select toolbar-select" value={coverage} onChange={(event) => setCoverage(event.target.value)}>
            <option value="all">All coverage</option>
            {coverages.map((item) => <option key={item} value={item}>{coverageLabel(item)}</option>)}
          </select>
          <select className="input input-select toolbar-select" value={kind} onChange={(event) => setKind(event.target.value)}>
            <option value="all">All venues</option>
            {kinds.map((item) => <option key={item} value={item}>{item.replaceAll('_', ' ')}</option>)}
          </select>
          <span className="toolbar-count"><Filter size={13} /> {filtered.length} / {venues.length}</span>
        </div>

        {filtered.length ? (
          <div className="venue-grid">
            {filtered.map((venue) => (
              <button className="venue-card" key={venue.id} onClick={() => setSelected(venue)}>
                <div className="venue-card-top">
                  <div>
                    <span className="venue-kind">{venue.kind.replaceAll('_', ' ')}</span>
                    <h3>{venue.name}</h3>
                  </div>
                  <Badge tone={coverageTone(venue.coverage)}>{coverageLabel(venue.coverage)}</Badge>
                </div>
                <div className="venue-program mono">{shortAddress(venue.program_id, 8, 8)}</div>
                <p>{venue.notes || COVERAGE_HELP[venue.coverage] || 'Registered liquidity venue.'}</p>
                <div className="venue-card-foot">
                  <span>{venue.decoder ? <><Code2 size={12} /> {venue.decoder}</> : <><Globe2 size={12} /> Jupiter fallback</>}</span>
                  {venue.math && <span>{venue.math}</span>}
                  <ArrowUpRight size={13} />
                </div>
              </button>
            ))}
          </div>
        ) : (
          <EmptyState icon={<Search size={28} />} title="No venues match" description="Clear the search or coverage filter to see the full registry." action={<Button variant="ghost" size="sm" onClick={() => { setSearch(''); setCoverage('all'); setKind('all') }}>Reset filters</Button>} />
        )}
      </Panel>

      <div className="grid-1-1">
        <Panel>
          <PanelHeader title="Coverage semantics" subtitle="What each badge guarantees" icon={<Boxes size={17} />} />
          <div className="coverage-legend">
            {Object.entries(COVERAGE_HELP).map(([key, text]) => (
              <div key={key}>
                <Badge tone={coverageTone(key)}>{coverageLabel(key)}</Badge>
                <span>{text}</span>
              </div>
            ))}
          </div>
        </Panel>
        <Panel>
          <PanelHeader title="Routing policy" subtitle="Fail closed at every boundary" icon={<Code2 size={17} />} />
          <div className="policy-list">
            <div><span>01</span><p>Local modeled and decoded venues are ranked first using fresh on-chain reserves.</p></div>
            <div><span>02</span><p>Jupiter is a quote fallback, never a source of unverified instruction layouts.</p></div>
            <div><span>03</span><p>Archived or deprecated programs never route; registration alone never enables execution.</p></div>
            <div><span>04</span><p>Every executable route must pass cost, impact, freshness and guard profit checks.</p></div>
          </div>
        </Panel>
      </div>

      <Modal
        open={selected !== null}
        onClose={() => setSelected(null)}
        title={selected?.name || 'Venue'}
        subtitle={selected ? `${selected.kind.replaceAll('_', ' ')} · ${selected.id}` : undefined}
        footer={<Button variant="ghost" onClick={() => setSelected(null)}>Close</Button>}
      >
        {selected && (
          <div className="venue-detail">
            <div className="detail-metrics">
              <div><span>Coverage</span><Badge tone={coverageTone(selected.coverage)}>{coverageLabel(selected.coverage)}</Badge></div>
              <div><span>Program</span><strong className="mono">{shortAddress(selected.program_id, 8, 8)}</strong></div>
              <div><span>Decoder</span><strong>{selected.decoder || 'Jupiter quote fallback'}</strong></div>
              <div><span>Math</span><strong>{selected.math || '—'}</strong></div>
            </div>
            <p className="venue-detail-note">{COVERAGE_HELP[selected.coverage]}</p>
            {selected.notes && <p className="venue-detail-note">{selected.notes}</p>}
            <div className="ref-list">
              <span>Program <code>{selected.program_id}</code><CopyButton value={selected.program_id} /></span>
              {selected.docs && (
                <span>
                  Documentation
                  <a href={selected.docs} target="_blank" rel="noreferrer"><BookOpen size={13} /> {selected.docs}</a>
                </span>
              )}
            </div>
          </div>
        )}
      </Modal>
    </div>
  )
}
