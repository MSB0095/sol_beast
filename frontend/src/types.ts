export type Mode = 'off' | 'radar' | 'canary' | 'live'

export interface FeedHealth {
  pools: number
  quotable: number
  stale: number
  last_refresh_ms: number | null
  age_ms: number | null
  last_error: string | null
}

export interface RiskReason {
  code: string
  detail: string
}

export interface RiskSnapshot {
  state: string
  reasons: RiskReason[]
  inflight: number
  landed_cost_today: number
  balance_floor_lamports: number
  daily_cost_cap_lamports: number
  kill_switch_file: string
}

export interface StoreStats {
  pools: number
  opportunities: number
  attempts: number
  landed: number
  reverted: number
  bundles: number
  events: number
  settlements: number
  realized_net_lamports: number
  last_event_ms: number | null
}

export interface StrategyMetrics {
  opportunities: number
  simulated: number
  not_landed: number
  landed: number
  reverted: number
  skipped: number
  attempts: number
  dropped: number
  realized_net_lamports: number
}

export interface MetricsSnapshot {
  started_at: string
  uptime_seconds: number
  feed_updates: number
  feed_dropped: number
  ws_reconnects: number
  rpc_errors: number
  opportunities: number
  simulated: number
  bundles_submitted: number
  bundles_landed: number
  bundles_dropped: number
  bundles_reverted: number
  guard_violations: number
  rent_closed_accounts: number
  rent_recovered_lamports: number
  unexpected_balance_delta_lamports: number
  realized_net_lamports: number
  last_slot: number
  last_update_ms: number
  feed_lag_ms: number
  per_strategy: Record<string, StrategyMetrics>
}

export interface GuardStatus {
  enabled: boolean
  mode: string
  program_id_configured: boolean
  min_profit_lamports: number
  unguarded_live_allowed: boolean
}

export interface FlashReserveStatus {
  reserve: string
  mint: string
  decimals: number
  available: number
  flash_fee_bits: number
  fee_model: 'fixed_point60_ceiling' | 'wad18_round' | 'no_fee' | 'bps16_ceiling'
  host_fee_percentage: number
  fee_ppm: number | null
}

export interface FlashStatus {
  provider: string
  program_id: string
  reserve: string | null
  mint: string | null
  decimals: number | null
  available: number | null
  flash_fee_bits: number | null
  fee_model: 'fixed_point60_ceiling' | 'wad18_round' | 'no_fee' | 'bps16_ceiling' | null
  host_fee_percentage: number | null
  fee_ppm: number | null
  max_borrow_lamports: number
  lamport_guard: boolean
  reserve_count: number
  reserves: FlashReserveStatus[]
}

export interface Status {
  service: string
  version: string
  uptime_seconds: number
  mode: Mode
  configured_mode: Mode
  real: boolean
  signing_enabled: boolean
  kill_switch: boolean
  feed: FeedHealth | null
  risk: RiskSnapshot | null
  store: StoreStats
  metrics: MetricsSnapshot
  jupiter_configured: boolean
  rent_configured: boolean
  flash: FlashStatus | null
  guard: GuardStatus
  jito_enabled: boolean
  api: { enabled: boolean; bind: string }
  wallet: { keypair_configured: boolean }
}

export interface StrategyState {
  name: string
  mode: Mode
  configured: boolean
  requires_bundle_guard: boolean
}

export interface OpportunityRecord {
  id: string
  strategy: string
  mode: string
  slot_detected: number
  detected_at_ms: number
  start_mint: string
  amount_in: number
  amount_out: number
  est_gross_lamports: number
  est_net_lamports: number
  confidence: number
  ttl_slots: number
  dedupe_key: string
  outcome: string
  legs_json: string
  costs_json: string
  meta_json: string
}

export interface Leg {
  venue: string
  pool: string
  mint_in: string
  mint_out: string
  amount_in: number
  min_out: number
  price_impact_bps: number
}

export interface CostBreakdown {
  tip_lamports: number
  priority_fee_lamports: number
  base_fee_lamports: number
  rent_lamports: number
  guard_lamports: number
  flash_fee_lamports?: number
}

export interface Attempt {
  id: string
  opportunity_id: string | null
  ts_ms: number
  strategy: string
  mode: string
  status: string
  bundle_id: string | null
  simulated: boolean
  landed: boolean
  reverted: boolean
  realized_net_lamports: number | null
  tip_lamports: number
  fee_lamports: number
  signature: string | null
  error: string | null
}

export interface Bundle {
  bundle_id: string
  attempt_id: string | null
  ts_ms: number
  status: string
  tip_lamports: number
  signatures_json: string
  landed_slot: number | null
  error: string | null
}

export interface Settlement {
  opportunity_id: string
  signature: string
  slot: number
  realized_gross_lamports: number
  realized_tip_lamports: number
  realized_fee_lamports: number
  realized_net_lamports: number
  modeled_net_lamports: number
  settled_at_ms: number
}

export interface EventRecord {
  id: number
  ts_ms: number
  level: string
  component: string
  message: string
  data_json: string
}

export interface StrategySummary {
  strategy: string
  opportunities: number
  profitable: number
  modeled_net_lamports: number
  best_net_lamports: number
}

export interface Dashboard {
  status: Status
  strategies: StrategyState[]
  venues: {
    count: number
    coverage: Record<string, number>
  }
  opportunities: OpportunityRecord[]
  attempts: Attempt[]
  bundles: Bundle[]
  settlements: Settlement[]
  events: EventRecord[]
  strategies_summary: StrategySummary[]
}

export interface Venue {
  id: string
  name: string
  program_id: string
  kind: string
  coverage: string
  decoder: string | null
  math: string | null
  docs: string | null
  notes: string | null
}

export interface VenuesResponse {
  count: number
  coverage: Record<string, number>
  venues: Venue[]
}

export interface PoolModel {
  [key: string]: unknown
}

export interface PoolSnapshot {
  address: string
  venue: string
  program_id: string
  mint_a: string
  mint_b: string
  vault_a: string
  vault_b: string
  decimals_a: number
  decimals_b: number
  fee_bps: number
  model: PoolModel
  slot: number
  confidence: number
}

export interface PoolsResponse {
  count: number
  source: string
  pools: PoolSnapshot[]
}

export interface RentAccount {
  address: string
  token_program: string
  mint: string
  owner: string
  amount: number
  lamports: number
  state: number
  closable: boolean
  reason: string
}

export interface RentScan {
  wallet: string
  scanned_at_ms: number
  accounts: RentAccount[]
  closable: number
  total_closable_lamports: number
  token_program_accounts: number
  token_2022_accounts: number
  errors: string[]
}

export interface RentBatch {
  signature: string | null
  accounts: string[]
  simulated: boolean
  recovered_lamports: number
  fee_lamports: number
  error: string | null
}

export interface RentReport {
  dry_run: boolean
  scanned: number
  closable: number
  attempted: number
  closed: number
  gross_recovered_lamports: number
  fee_lamports: number
  net_recovered_lamports: number
  batches: RentBatch[]
  errors: string[]
}
