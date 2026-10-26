# 🦁 SOL BEAST 2.0 — "Green-or-Revert" Keeper & MEV Engine

### Full product, engineering and operations roadmap

| Field | Value |
|---|---|
| Document version | 1.0 |
| Date | 2026-10-25 |
| Target repo | `github.com/MSB0095/sol_beast` (branch `master`, 129 commits at review time) |
| Status | Design proposal + shipped v2 subset (see implementation status) |
| Horizon | ~28 weeks part-time (see §6), gated by data, not by calendar |
| Scope | Wallet-capital arbitrage, rent recovery, guard/Jito execution; flash, liquidations and backrunning are design-only |

**Legend used throughout**

- 🔎 **VERIFY** — fast-moving fact (fees, program IDs, endpoints, integrations). Confirm against current docs/IDLs before coding.
- ⚠️ **RISK** — a way this can lose money, time or keys.
- ✅ **DoD** — definition of done / acceptance criteria.
- 🚦 **GATE** — a go/no-go decision point based on measured data.
- Effort sizes: **S** ≤ 1 dev-day, **M** 2–3, **L** 4–6, **XL** 7+.

> **Implementation status (2026-10-26).** Phases 0–2 and the risk, data,
> control-plane and rent pieces needed for v2 are shipped. Phase 3 (flash),
> Phase 4 (flash-arbitrage), Phase 5 (liquidations) and Phase 6
> (backrun/oracle-lag) are **not shipped**; those sections are design plans
> only. The delivered engine, API and guard are documented in
> [docs/V2.md](docs/V2.md).

---

## Table of contents

1. Vision, principles, non-goals
2. Constraints and assumptions
3. Current-state audit of the repo
4. Target architecture
5. Core economics (cost model, tips, bankroll, infra tiers)
6. Roadmap overview and timeline
7. Phase 0 — Foundations and hygiene
8. Phase 1 — Radar, telemetry and rent reclaimer
9. Phase 2 — Execution core (bundles, guard, simulation)
10. Phase 3 — Flash-loan layer
11. Phase 4 — Flash-arbitrage strategies
12. Phase 5 — Liquidation engine
13. Phase 6 — Streaming-dependent strategies (backrun, Express Relay, oracle-lag)
14. Phase 7 — Risk engine and safety
15. Phase 8 — Dashboard and control plane
16. Phase 9 — Data, persistence and analytics
17. Phase 10 — Operations, infrastructure, deployment
18. Phase 11 — Off-chain income tracks (bounties, research lab, public tools)

Testing/QA, configuration/API reference, data model, KPIs, risk register,
threat model, compliance, icebox, 14-day plan and appendices are tracked in
[docs/V2.md](docs/V2.md) and the repository CI, not as unwritten roadmap
sections.

---

## 1. Vision, principles, non-goals

### 1.1 Vision

SOL BEAST today is a pump.fun token sniper: it watches launches, buys with the owner's SOL, and exits on take-profit / stop-loss / timeout. **SOL BEAST 2.0 keeps the soul of the project — Rust speed, real-time dashboard, dry mode, risk controls, cyberpunk skin — and adds a second engine that earns without risking principal.** Every strategy in the new engine is built so that a transaction either:

1. lands **and** ends with a verified net profit in the wallet, or
2. **does not land at all** (no fee, no tip, no loss).

### 1.2 Principles

1. **Green-or-revert.** Profit is enforced on-chain (flash-loan repayment + minimum-profit guard) and landing is enforced by bundle semantics (all-or-nothing, tip paid only if the bundle lands).
2. **Measure before spending.** Every strategy ships first in *Radar* mode (detect + model + log, never send). Live mode is unlocked per strategy by data gates (§23).
3. **Capital-free by construction.** No strategy may require holding inventory risk. Anything that needs capital (market making, LP, basis trades, perps collateral) is out of scope.
4. **Dry mode is the default.** Same as today: no `--real`, no transactions.
5. **One abstraction, many strategies.** A `Strategy` trait + shared execution core, so adding strategy N+1 is cheap.
6. **Observable by default.** Every opportunity gets an ID and a full lifecycle record (detected → modeled → simulated → submitted → landed/dropped → settled).
7. **Fail closed.** Circuit breakers, balance floors, kill switch. When in doubt, don't send.
8. **Legit only.** No sandwiching, no front-running of user orders, no oracle manipulation, no wash trading, no exploits of unaudited bugs.

### 1.3 Non-goals

- Sandwich / frontrun strategies (harmful to users; BAM and protocol changes are also eroding them).
- Anything that needs inventory or collateral: market making, CLMM LPing, funding/basis trades, perp liquidation on venues that require the liquidator to post margin.
- Cross-chain arbitrage (not atomic, needs capital on both sides).
- Airdrop farming and points farming.
- Running a validator (high fixed cost; see Icebox).
- Memecoin speculation on the owner's capital (the legacy sniper remains available but is *not* part of the roadmap's profit model).

---

## 2. Constraints and assumptions

### 2.1 Hard constraints (from the owner)

| # | Constraint | Consequence for design |
|---|---|---|
| C1 | Only gas SOL available, nothing to invest | All principal must come from flash loans; wallet balance is for fees, tips (only if landed) and refundable rent |
| C2 | Transaction must be profitable or not succeed | Bundle-only submission + on-chain profit guard; never use plain `sendTransaction` for strategy txs |
| C3 | Minimal fees | Bundles (no fee for dropped bundles), simulation-first, local math, no spam |
| C4 | Legitimate strategies only | Explicit non-goals (§1.3); ethics section (§26) |
| C5 | No airdrops | Not in scope; no points/airdrop tracking features |
| C6 | "Geeky" strategies | Flash-loan arb, liquidations, keeper bots, MEV infrastructure, security research |

### 2.2 Working assumptions

- A1. One primary developer (the owner) plus AI assistance; ~12–20 focused hours per week. Estimates below are in *dev-days* of focused work.
- A2. The owner will accept **some fixed infrastructure cost** if a data gate proves an edge. Phase 1 can run on free tiers; competitive execution cannot (see §5.4).
- A3. Solana mainnet-beta is the only live target. Devnet is used for instruction-shape tests; mainnet-fork (Surfpool / LiteSVM snapshots) for realistic tests.
- A4. Competition is strong: Jito tips dominate priority-fee volume and the best searchers run co-located bare metal. Reports for Q2 2026 show Jito tip volume roughly halving quarter-over-quarter, i.e. **margins are thin and shrinking**. The roadmap therefore leads with measurement (Radar) and niche selection (long-tail pools, smaller markets), not head-on competition.
- A5. Fast-moving ecosystem facts (lending-market share, flash-loan fees, BAM adoption, Alpenglow timing, program IDs) change; every such fact is tagged 🔎 and re-verified at the start of the relevant phase.

---

## 3. Current-state audit of the repo

### 3.1 What the repo is

A Rust (Tokio + Axum) pump.fun sniper with a React/TypeScript dashboard (7 themes), a Helius Sender integration, dry mode, TP/SL/timeout position management, and a small docs site. MIT licensed. ~8,400 lines of Rust across 16 files.

### 3.2 File map (line counts at review)

| File | Lines | Role | 2.0 disposition |
|---|---:|---|---|
| `src/main.rs` | 1879 | runtime, task spawning, token handling pipeline | Split; sniper pipeline moves to `beast-sniper`; supervisor moves to `beast-core` |
| `src/rpc.rs` | 1705 | RPC + buy/sell logic | Split: generic RPC helpers → `beast-exec`; pump.fun buy/sell → `beast-sniper` |
| `src/api.rs` | 605 | Axum REST API | Extend, harden, move to `beast-api` |
| `src/idl.rs` | 579 | Anchor IDL parsing | **Reuse** → generic instruction builder for Kamino/MarginFi/etc. |
| `src/buyer.rs` | 502 | buy heuristics | Stays with sniper |
| `src/ws.rs` | 500 | pump.fun log WebSocket with reconnect | Reuse reconnect logic in `beast-feed` fallback adapter |
| `src/settings.rs` | 440 | config structs + merge logic | Replace with layered config v2 |
| `src/helius_sender.rs` | 435 | Helius Sender submission, tip floor, priority fee | **Reuse** tip-floor / priority-fee helpers; *not* used for strategy txs (not bundle-semantic) |
| `src/monitor.rs` | 385 | holdings TP/SL monitor | Stays with sniper |
| `src/tx_builder.rs` | 331 | pump.fun buy/sell instruction builders | Stays with sniper |
| `src/models.rs` | 341 | data structures | Split per crate |
| `src/pumpportal.rs` | 315 | PumpPortal feed | Reuse for new-mint discovery |
| `src/onchain_idl.rs` | 313 | on-chain IDL fetch | **Reuse** for lending/DEX IDLs |
| `src/dev_fee.rs` | 60 | 1% dev-fee transfer instruction | See §7 (P0-9) |
| `src/error.rs` | 24 | error type | Expand |
| `src/state.rs` | 17 | `BuyRecord` | Stays with sniper |
| `idl/` | — | `pumpfun.json`, `pumpfunamm.json`, `pumpfunfees.json` | PumpSwap IDL directly reusable for arb legs |
| `tools/pumpportal_listen.py` | — | feed debugging script | Keep |

### 3.3 Dependencies (Cargo.toml highlights)

`tokio` (full), `tokio-tungstenite`, `reqwest`, `solana-program/client/sdk/account-decoder 2.1.1`, `borsh`, `spl-token 6`, `spl-associated-token-account 4`, `mpl-token-metadata 5.1.1`, `axum 0.7` (+ws), `tower-http`, `tracing`, `lru`, `config`, `toml`, `bincode`.

### 3.4 Strengths to preserve

- Clean async foundation (Tokio), WebSocket auto-reconnect, regional endpoint selection (`select_healthy_wss`).
- Existing IDL tooling — a major accelerator for building lending-protocol instructions.
- Dry mode and the `--real` switch culture.
- A working REST + live dashboard pattern; themeable UI.
- Dynamic Jito tip-floor fetch (75th percentile) and priority-fee estimation.

### 3.5 Gaps relative to the 2.0 goals

| # | Gap | Impact | Addressed in |
|---|---|---|---|
| G1 | Strategy txs would go through Helius Sender (plain transactions, dual-routed). A reverted tx can still land and cost base + priority fee. | Violates C2/C3 | P2 |
| G2 | No account/slot/transaction streaming (only pump.fun logs via WS + PumpPortal). | Cannot detect arbs, liquidations, backruns competitively | P1, P6 |
| G3 | No flash-loan instruction builders, no profit guard, no ALT management. | Cannot build capital-free txs | P2, P3 |
| G4 | Blocking `solana_client::rpc_client::RpcClient` is imported in `helius_sender.rs`. | 🔎 Verify usage; blocking calls on the hot async path add latency/jitter | P0-6 |
| G5 | `main.rs` (1.9k lines) and `rpc.rs` (1.7k lines) are monoliths. | Slows every future change | P0-2 |
| G6 | Config updates "require restart"; settings accept raw private-key strings. | Hot-reload and key-hygiene gaps | P0-3, P0-4, P7 |
| G7 | API hardening unknown (🔎 verify auth/binding/CORS in `api.rs`). | A trading bot's control API must not be exposed unauthenticated | P0-8 |
| G8 | No persistent store (in-memory + logs). | No post-mortems, no analytics, no tax export | P9 |
| G9 | No automated tests beyond `cargo test` placeholders; no CI. | High regression risk when adding money-touching code | P0-1, §19 |
| G10 | Sniper buy path spends owner capital by design. | Conflicts with C1 | Kept dry-by-default, isolated in its own crate |
| G11 | 1% dev fee defaults to **enabled** on sniper buys/sells to a hardcoded wallet. | Must be an explicit, documented owner decision; must never apply to new strategies | P0-9 |

---

## 4. Target architecture

### 4.1 Data flow

```
 ┌──────────────────────────────── FEEDS ─────────────────────────────────┐
 │ Yellowstone gRPC (accounts, slots, txs)  ─┐                            │
 │ ShredStream (optional, Tier 2)  ──────────┤                            │
 │ Pyth/Hermes price stream  ────────────────┤──►  StateCache  ──► Detectors (one per strategy)
 │ PumpPortal / pump.fun logs (existing) ────┤        │                    │
 │ Jupiter quote API (self-hosted)  ─────────┘        │                    ▼
 └─────────────────────────────────────────────────────┘          Opportunity Bus (mpsc / broadcast)
                                                                            │
                                                                            ▼
                                                  Evaluator: cost model · sizing · risk gates
                                                                            │
                                                                            ▼
                                       Tx Builder: flash wrap · ALTs · CU tuning · guard · tip
                                                                            │
                                                                            ▼
                                       Simulator: local math → RPC simulate → bundle simulate
                                                                            │
                                                                            ▼
                                       Submitter: Jito multi-region · BAM client · fallbacks
                                                                            │
                                                                            ▼
                                       Settlement tracker: landed? realized profit? reconcile
                                                                            │
                                         ┌──────────────┬───────────────────┴──────────┐
                                         ▼              ▼                              ▼
                                      SQLite        Prometheus                Dashboard / Telegram
```

### 4.2 Cargo workspace layout

```
sol_beast/
├─ Cargo.toml                 # [workspace]
├─ rust-toolchain.toml
├─ crates/
│  ├─ beast-core/             # types, config v2, errors, cost model, risk traits, supervisor, clock
│  ├─ beast-feed/             # gRPC / WS / shred ingestion + StateCache (isolated solana dep versions)
│  ├─ beast-dex/              # decoders + local math: Raydium, Orca, Meteora, PumpSwap, Phoenix, stake pools, stable-swap
│  ├─ beast-lend/             # decoders + health math + ix builders: Kamino, MarginFi, Jupiter Lend, Save
│  ├─ beast-flash/            # FlashProvider implementations
│  ├─ beast-exec/             # tx builder, ALT manager, CU tuner, guard ix, Jito client, simulator, submitter
│  ├─ beast-strategies/       # Strategy implementations (arb, liquidator, backrun, reclaimer, ...)
│  ├─ beast-sniper/           # legacy pump.fun sniper (dry by default)
│  ├─ beast-store/            # SQLite persistence + migrations
│  ├─ beast-api/              # Axum REST + WS
│  └─ beast-cli/              # binary: radar | run | reclaim | backtest | replay | doctor
├─ programs/
│  └─ guard/                  # tiny on-chain profit-guard program (pinocchio or Anchor)
├─ frontend/                  # React dashboard (existing)
├─ docs/                      # docs site + ADRs
├─ fixtures/                  # account snapshots, recorded feeds, golden tests
├─ tools/                     # scripts (pumpportal_listen.py, replay helpers)
└─ ops/                       # systemd units, docker, Grafana dashboards, alert rules
```

**Why isolate `beast-feed`:** Yellowstone gRPC client crates are pinned to specific `solana-*` versions. Keeping the feed in its own crate that emits *plain structs* (pubkey bytes, slot, data `Vec<u8>`) avoids dependency-version conflicts with the rest of the workspace.

### 4.3 Runtime model

- One Tokio runtime; hot-path tasks pinned to dedicated worker threads where useful (feed decode, detector, builder/signer).
- A **supervisor** owns all long-running tasks: restart with exponential backoff, propagate `CancellationToken`, expose health to the API.
- Channels: `broadcast` for state updates (many detectors), `mpsc` for opportunities (single evaluator), bounded with drop-oldest policy on the hot path (stale opportunities are worthless).
- **Slot clock**: derived from the gRPC slot stream; exposes current slot, slot time, upcoming leaders, whether the next leaders run Jito/BAM.
- **Blockhash cache**: refreshed per slot at `confirmed`; each built tx records the blockhash + last-valid height.

### 4.4 Core types (sketch)

```rust
pub enum StrategyKind {
    FlashArb, Liquidation, Backrun, ExpressRelay, OracleLag,
    RentReclaim, ProfitSweep,
}

pub struct Opportunity {
    pub id: Uuid,
    pub strategy: StrategyKind,
    pub slot_detected: u64,
    pub detected_at: Instant,
    pub legs: Vec<LegPlan>,          // abstract plan, not yet instructions
    pub borrow: Option<BorrowPlan>,  // mint, amount, provider
    pub est_gross_lamports: i64,     // in lamports-equivalent
    pub est_costs: CostBreakdown,    // fees, tip, rent amortization
    pub est_net_lamports: i64,
    pub confidence: f32,             // model confidence 0..1
    pub ttl_slots: u8,               // how long the plan stays valid
    pub meta: serde_json::Value,     // strategy-specific context for the ledger
}

#[async_trait]
pub trait Strategy: Send + Sync {
    fn kind(&self) -> StrategyKind;
    fn name(&self) -> &'static str;
    fn mode(&self) -> Mode;                         // Off | Radar | Canary | Live
    async fn on_update(&mut self, u: &StateUpdate, ctx: &Ctx) -> Vec<Opportunity>;
    async fn build(&self, opp: &Opportunity, ctx: &Ctx) -> Result<BuiltTx>;
    fn on_result(&mut self, res: &SettlementResult);  // learning hooks (tip shading, cooldowns)
}

pub enum Mode { Off, Radar, Canary, Live }
```

`Canary` = live with micro limits (tiny max tip, max one attempt per N seconds, strict daily cost cap) used for the first live runs of each strategy.

---

## 5. Core economics

### 5.1 Cost model

For every opportunity, all quantities in lamports (convert token-denominated profit using the quote of the profit token to SOL):

```
gross_profit      = out_amount − (borrow_amount + flash_fee)          # leg-level, after DEX fees (DEX fees already in quotes)
variable_costs    = jito_tip + priority_fee + base_fee(5_000 × n_sigs)
fixed_amortized   = ata_rent_new_accounts × refund_probability_adj + guard_cu_cost
net_profit        = gross_profit − variable_costs − fixed_amortized
EV(attempt)       = p_land × net_profit − (1 − p_land) × cost_if_not_landed
```

For **bundle-only** submission, `cost_if_not_landed ≈ 0` (dropped bundles pay no fee and no tip). Therefore the expected value of an attempt is non-negative whenever `net_profit ≥ 0` is enforced on-chain, and the *real* cost of failing is infrastructure and opportunity cost, not lamports.

> ⚠️ **Repay-only is not a profit guard.** A flash loan reverts if you cannot repay `amount + fee`, so it enforces `gross_profit ≥ 0`. It does **not** enforce `gross_profit ≥ tip + fees`. A tx that nets 1 lamport of gross profit but pays a 50,000-lamport tip would land and lose money. The guard must assert `final_balance ≥ start_balance + tip + fees + min_profit`.

### 5.2 Tip policy

- `tip = clamp(α × est_gross, tip_floor, tip_cap)` with `α ∈ [0.30, 0.90]` initial default 0.50.
- `tip_floor` = max(1,000 lamports, current Jito tip-floor percentile chosen by strategy). `tip_cap` = per-strategy config.
- Per-opportunity-class **bid shading**: record (α, landed?) pairs per strategy × size bucket; fit a simple logistic win-probability curve `p_land(α)` and choose α maximizing `p_land(α) × (1−α) × gross`. Start with grid exploration in Canary mode (ε-greedy), then exploit.
- Tip is included **in the same transaction** as the strategy logic (last instruction) to avoid unbundling/"uncle" risk, and is counted inside the guard.
- Skip the opportunity if `net_profit < min_net_profit_lamports` after tipping.

### 5.3 Bankroll plan (gas-only)

Fixed lamport facts (stable protocol constants):

| Item | Lamports | SOL |
|---|---:|---:|
| Base fee per signature | 5,000 | 0.000005 |
| Minimum Jito tip | 1,000 | 0.000001 |
| SPL token account rent (165 bytes) | 2,039,280 | 0.00203928 |
| Durable nonce account rent (80 bytes) | 1,447,680 | 0.00144768 |
| Rent per byte (rent-exempt, 2 years) | 6,960 | — |

Rent-exempt minimum = `(128 + data_len) × 3,480 × 2` lamports.

**Starter wallet budget (example 0.30 SOL):**

| Use | SOL | Notes |
|---|---:|---|
| wSOL ATA | 0.0020 | refundable |
| ATAs for ~10 tradable mints | 0.0204 | refundable; only for mints actually used |
| Tip + fee float | 0.0300 | only spent if bundles land (paid from profit in practice) |
| ALT creation/extension rent | 0.0100 | refundable on close |
| Guard program deploy (optional, later) | 0.0400–0.1500 | refundable if program is later closed; a ~5 KB `pinocchio` program needs ≈0.04 SOL for program data, plus a temporary buffer of similar size during deploy |
| Safety reserve | remainder | never touched by strategies (balance floor, §14) |

Rule: **no strategy may ever spend below `balance_floor`**; the engine reads the floor from config and enforces it in the evaluator.

### 5.4 Infrastructure tiers (the real "capital")

Prices are rough ranges as of late 2026 🔎 verify before committing.

| Tier | What you get | Approx. monthly cost | Suitable for |
|---|---|---:|---|
| **0 — Free** | Free-tier RPC/WS, public Jupiter lite API, SQLite, laptop | $0 | Radar mode, backtests, rent reclaimer, bounty radar, slow long-tail liquidations |
| **1 — Starter** | Paid RPC with WS + Jito-aware sending, small VPS in a Jito region (Frankfurt/Amsterdam/NY/Tokyo), possibly gRPC add-on | ~$30–200 | First live Canary runs; long-tail arbs; liquidations |
| **2 — Competitive** | Dedicated Yellowstone gRPC / LaserStream-style feed, bare-metal in a hub, ShredStream, self-hosted Jupiter swap API | ~$300–1,500 | Contested arbs, backruns |
| **3 — Sovereign** | Own validator/RPC node | $1,000+ | Not recommended (Icebox) |

🚦 **Infra gate:** do not step up a tier until the previous tier's Radar data shows `theoretical_net × realistic_win_rate × 30 days > 2 × next_tier_monthly_cost`.

### 5.5 Break-even sketch

If a strategy lands `N` bundles/day with average net `x` SOL after tips and fees, monthly profit ≈ `30·N·x − infra`. Example: Tier 1 at $100/month with SOL at $200 → 0.5 SOL/month fixed. At `x = 0.002 SOL`, you need ≈ 8 landed bundles/day just to cover infra. Radar mode exists to estimate `N` *before* paying for infra.

---

## 6. Roadmap overview and timeline

### 6.1 Phases at a glance

| Phase | Name | Weeks (part-time) | Effort | Depends on | Primary exit gate |
|---|---|---|---|---|---|
| P0 | Foundations and hygiene | 1–2 | ~8 dev-days | — | 🚦G0 CI green, workspace builds, API hardened |
| P1 | Radar, telemetry, rent reclaimer | 2–6 | ~16 | P0 | 🚦G1 Radar shows a measurable modeled edge |
| P2 | Execution core | 5–10 | ~20 | P0 | 🚦G2 Canary bundle lands with verified guard |
| P3 | Flash-loan layer | 8–12 | ~14 | P2 | all 4 providers pass fork tests |
| P4 | Flash-arbitrage strategies | 10–16 | ~24 | P1,P2,P3 | 🚦G3 first strategy Live with positive realized P&L |
| P5 | Liquidation engine | 14–20 | ~24 | P1,P2,P3 | 🚦G4 first liquidation landed |
| P6 | Streaming strategies | 18–24 | ~20 | P1,P2, infra Tier 2 | backrun/Express Relay Canary |
| P7 | Risk engine and safety | 6–24 (continuous) | ~10 | P2 | circuit breakers proven in chaos tests |
| P8 | Dashboard and control plane | 3–26 (continuous) | ~18 | P1 | all strategy cards live |
| P9 | Data, persistence, analytics | 4–12 | ~8 | P1 | replay + tax export working |
| P10 | Ops, infra, deployment | 8–28 | ~10 | P2 | 7-day unattended soak test |
| P11 | Off-chain income tracks | any time | ~10 | — | first submission to a bounty/contest |

Total ≈ 160 dev-days (~28 weeks at 12–20 h/week). The critical path is **P0 → P1 → P2 → P3 → P4**; everything else parallelizes.

### 6.2 Gantt (ASCII)

```
Week            1   3   5   7   9   11  13  15  17  19  21  23  25  27
P0 Foundations  ████
P1 Radar            ████████
P2 Exec core            ██████████
P3 Flash layer                  ████████
P4 Flash arbs                       ████████████
P5 Liquidator                               ████████████
P6 Streaming                                        ████████████
P7 Risk                 ░░░░░░░░░░░░░░░░░░░░░░░░░░░░
P8 Dashboard        ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
P9 Data                 ██████████
P10 Ops                         ░░░░░░░░░░░░░░░░░░░░░░
P11 Off-chain   ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ (opportunistic)
```
(█ = focused build, ░ = continuous / background)

### 6.3 Gates overview

| Gate | Where | Question it answers | Pass condition (default) |
|---|---|---|---|
| G0 | End P0 | Is the foundation safe to put money-touching code on? | CI green; API auth on; no raw private keys accepted in live mode; workspace builds |
| G1 | End P1 | Is there a modeled edge worth building for? | ≥ 14 days of Radar data; ≥ 1 strategy with modeled net ≥ 0.05 SOL/day *after* a pessimistic 5% win-rate assumption |
| G2 | End P2 | Does revert-protection actually work end-to-end? | ≥ 20 Canary bundles built; 100% of dropped bundles cost 0 lamports; ≥ 1 landed bundle with realized profit = predicted ± 5% |
| G3 | P4 | Is the first strategy profitable live? | 14-day Canary/Live: realized net P&L > 0 and landed-cost ≤ cap |
| G4 | P5 | Is the liquidator competitive? | ≥ 1 landed liquidation or a documented reason (e.g. all positions taken <1 slot) with a decision to continue/stop |
| G5 | Anytime | Should infra spend step up a tier? | See §5.4 formula |

---

## 7. Phase 0 — Foundations and hygiene (Weeks 1–2)

**Goal:** make the repo safe and fast to change before any money-touching code is added.

### 7.1 Work items

| ID | Task | Details | Effort | Deps |
|---|---|---|---|---|
| P0-1 | Baseline and CI | Tag `v0.1-sniper`; add GitHub Actions: `cargo fmt --check`, `clippy -D warnings`, `cargo test`, `cargo deny` (licenses/advisories), `cargo audit`; frontend `npm run lint` + type-check; cache `target/` | S | — |
| P0-2 | Convert to Cargo workspace | Create crates per §4.2; move code with `git mv` to preserve history; keep a `beast-sniper` binary alias `sol_beast` so existing scripts still work; feature-flag sniper (`--features sniper`) | L | P0-1 |
| P0-3 | Config v2 | Layered: `config.default.toml` < `config.toml` < env (`BEAST_*`) < CLI; `#[serde(deny_unknown_fields)]`; semantic validation (sum of TP percentages, tip caps, floors); `config_version` + migrations; per-strategy `[strategies.<name>]` tables; secrets split into `secrets.toml` / env | M | P0-2 |
| P0-4 | Wallet and key hygiene | Only `wallet_keypair_path` (0600 perms checked at startup) or an env-injected keypair for **live** mode; reject `wallet_private_key_string` in live; `Signer` abstraction (file, env, later remote signer); `zeroize` on drop; separate **roles**: `hot` (strategy gas wallet), `sniper` (legacy), `cold` (sweep target, pubkey only) | M | P0-3 |
| P0-5 | Errors and logging | Extend `error.rs` into a taxonomy (`Feed`, `Rpc`, `Sim`, `Build`, `Submit`, `Risk`, `Config`); `tracing` spans carry `strategy`, `opportunity_id`, `slot`; JSON log option; log redaction for keys/URLs with API tokens | S | P0-2 |
| P0-6 | Async hygiene | 🔎 Audit every use of blocking `RpcClient` (imported in `helius_sender.rs`); switch to `solana_client::nonblocking` or `spawn_blocking`; remove `block_on` in async paths; central `HttpClient` with connection pooling, HTTP/2, timeouts; graceful shutdown with `CancellationToken` | M | P0-2 |
| P0-7 | Supervisor | Task supervisor with restart/backoff, health states (`Starting/Healthy/Degraded/Failed`), panic capture, exposes `/api/health/tasks` | M | P0-6 |
| P0-8 | API hardening | Bind `127.0.0.1` by default; bearer-token auth (token in secrets); strict CORS allowlist; request-size limits; rate limiting (`tower`); never return secrets; separate read-only token for the dashboard viewer; audit log of config changes | M | P0-3 |
| P0-9 | Dev-fee policy | Decide and document: keep as an opt-in for the sniper only, or remove. New strategies **never** call `dev_fee`. If kept, change default to **disabled**, show the setting prominently in the dashboard, and log every dev-fee transfer. (Margins in MEV work are too thin for an invisible 1% skim, and hidden defaults erode trust in an MIT project.) | S | P0-3 |
| P0-10 | Test harness skeleton | `fixtures/` with account snapshots; Mollusk/LiteSVM helpers; golden-file tests for instruction serialization; `cargo nextest` | M | P0-2 |
| P0-11 | Docs scaffolding | `docs/adr/` (architecture decision records), `docs/runbooks/`, `CONTRIBUTING.md`, `SECURITY.md`, issue/PR templates | S | — |
| P0-12 | `beast doctor` | CLI that checks: config validity, wallet perms, RPC/gRPC reachability and latency, Jito endpoints, clock skew, disk, versions | S | P0-3 |

### 7.2 ✅ DoD (G0)

- CI is green on `master`; workspace builds in < 5 min cached.
- `beast doctor` passes on a clean machine.
- API refuses unauthenticated requests; verified by an integration test.
- Live mode refuses raw private keys in config; verified by a test.
- A written ADR records the dev-fee decision.

### 7.3 Risks

- ⚠️ Refactor regressions in the sniper → mitigate with golden tests on tx builders before moving files.
- ⚠️ Solana crate churn (the monolithic `solana-sdk` is being split into smaller crates) → keep Solana-typed code behind thin adapters in `beast-exec`.

---

## 8. Phase 1 — Radar, telemetry and rent reclaimer (Weeks 2–6)

**Goal:** know where the edge is (or isn't) **before** paying for infrastructure or writing execution code. Ship one immediately useful feature (rent reclaimer) so the project earns something while the research runs.

### 8.1 Radar concept

Radar mode runs the full detect → model → size → cost pipeline against live data and logs *hypothetical* trades. It never builds a signed transaction for submission. Each record includes the modeled gross/net, the data age, and — crucially — a **competitor-outcome label** determined by watching what actually happened on-chain afterwards:

| Label | Meaning |
|---|---|
| `open_expired` | opportunity vanished without an observed taker (price moved naturally) |
| `taken_same_slot` | spread closed in the same slot we detected it |
| `taken_next_slot` | closed within 1–2 slots (we'd have had a chance with faster infra) |
| `taken_late` | persisted ≥ 3 slots before being taken (very winnable) |
| `never_taken` | persisted until TTL (stale data or non-executable) |

When the taker is identifiable (tx observed via gRPC), decode their **tip** (SOL transfer to a Jito tip account) and **realized profit** → builds a competitor tip distribution per strategy and size bucket. That distribution feeds the tip model in Phase 2.

### 8.2 Work items

| ID | Task | Details | Effort | Deps |
|---|---|---|---|---|
| P1-1 | Core traits & bus | `Strategy`, `Opportunity`, `Mode`, `StateUpdate`, bounded channels, evaluator skeleton (§4.4) | M | P0 |
| P1-2 | Feed adapters | (a) **WS fallback** (`accountSubscribe`, `slotSubscribe`, `logsSubscribe`) with the existing reconnect logic — Tier 0; (b) **Yellowstone gRPC** adapter — Tier 1+; same `StateUpdate` output; backpressure + lag metric | L | P1-1 |
| P1-3 | StateCache | Keyed by pubkey → `(slot, write_version, data)`; LRU for cold accounts, pinned set for hot pools; derived-state cache (pool reserves, tick arrays, obligation health); memory budget metrics | M | P1-2 |
| P1-4 | Pool universe selector | Build a candidate set: top N pools by TVL/volume from on-chain scan + public aggregator data; token allow/deny lists; Token-2022 extension screening (§11.8); refresh daily | M | P1-3 |
| P1-5 | DEX decoders + math (v1) | Raydium CPMM and AMM v4, PumpSwap, Orca Whirlpool (tick-array walk), Meteora DLMM (bin walk) and DAMM; Phoenix (book top-of-book) optional. Each exposes `quote_exact_in(mint_in, amount) -> out` offline; fuzz against on-chain simulation | XL | P1-3 |
| P1-6 | Route finder | Graph of tokens (nodes) and pools (edges); `−ln(rate)` weights; negative-cycle search limited to 2–3 hops; incremental recompute on pool change; start-token restricted to SOL/USDC/USDT | L | P1-5 |
| P1-7 | Sizing | Closed-form optimal input for two-pool constant-product pairs; golden-section search for CLMM/DLMM; cap by flash liquidity, pool depth (max price impact), and tx size | M | P1-5 |
| P1-8 | Cost model v1 | §5.1 implemented as a pure function with unit tests; Jito tip-floor percentile feed; priority-fee estimator (reuse `get_priority_fee_estimate`) | S | P1-1 |
| P1-9 | Radar logger + labels | Writes `opportunities` rows (§22) with the competitor-outcome labelling process (watch pool accounts for N slots after detection) | M | P1-3, P9-1 |
| P1-10 | Record / replay | `beast record --out fixtures/feed.zst` and `beast replay` deterministic playback of the feed into the pipeline; used for regression and strategy A/B tests | M | P1-2 |
| P1-11 | Rent reclaimer v1 | See §8.3 | M | P0 |
| P1-12 | Radar reports | Daily Markdown + CSV: opportunity count, distribution of modeled net, outcome labels, estimated competitor tips, "what-if win-rate" table | S | P1-9 |
| P1-13 | Dashboard Radar tab | Live opportunity feed, funnel counters, per-strategy modeled P&L | M | P8 |
| P1-14 | Latency prober | Periodic RTT/age probes for RPC, gRPC, Jito regions, Jupiter API; auto-pick best; extends `select_healthy_wss` | S | P1-2 |

### 8.3 Rent reclaimer (first money-making feature)

**What it reclaims**

| Account type | Typical rent | Notes |
|---|---:|---|
| Empty SPL token accounts | ~0.00204 SOL | The sniper leaves one per token traded |
| Dust token accounts | ~0.00204 SOL | burn dust then close; opt-in, value-thresholded |
| Token-2022 accounts | slightly more | withheld transfer fees must be harvested first; some extensions block closing |
| Old program buffers | varies (can be large) | `solana program close --buffers`-equivalent |
| Durable nonce accounts | ~0.00145 SOL | withdraw to owner |
| Fully deactivated stake accounts | rent + balance | withdraw |
| OpenBook open-orders accounts | ~0.02 SOL class | 🔎 close if empty |
| Empty DEX position accounts | varies | Orca/Raydium positions with zero liquidity — close/burn NFT |

**Behavior**

1. Scan the wallet's accounts (`getTokenAccountsByOwner` for both token programs + `getProgramAccounts` filters for buffers/nonces/stake).
2. Classify: `closable_empty`, `closable_after_burn` (value below threshold per price source), `blocked` (frozen, extension-locked, non-zero valuable balance).
3. Build batches (≈ 15–25 `CloseAccount` ixs per tx depending on size), simulate, send.
4. Dashboard shows "Rent recovered: X SOL" with a per-run history.
5. **Safety:** default allowlist of "close only if balance == 0"; dust-burn is opt-in with a USD threshold and a confirmation step; NFT/high-value tokens are never burned; dry-run prints exactly what would happen.

**Optional public tool (Phase 11):** a web page where *any* user connects a wallet, sees closable accounts, and signs batches; a transparent service fee (e.g. 10–15% of reclaimed rent) is added as a separate visible transfer. Consent-based only — the tool never touches accounts it cannot show.

### 8.4 ✅ DoD (G1)

- Radar runs ≥ 14 consecutive days on Tier 0/1 infra with < 1% data-gap time.
- `beast radar report` produces the daily report; includes competitor tip distribution for at least one strategy.
- Rent reclaimer recovers ≥ 95% of the closable rent in a test wallet and never closes a non-empty valuable account (tested on a fixture wallet).
- A written **Edge Memo** (1–2 pages) states, per strategy: opportunity rate, modeled net distribution, winnable fraction (`taken_late` + `never_taken`), and a recommendation (build / niche-only / drop).

### 8.5 Risks

- ⚠️ Local math diverges from on-chain reality (fee tiers, tick rounding) → mitigate with continuous cross-checks (`simulateTransaction` samples) and a "model error" metric.
- ⚠️ Free-tier WS lag inflates `taken_same_slot` → label includes feed-age; compare Tier 0 vs Tier 1 samples on the same window.

---

## 9. Phase 2 — Execution core (Weeks 5–10)

**Goal:** a single, well-tested pipeline that turns an `Opportunity` into a bundle that **lands profitable or doesn't land**.

### 9.1 Components

#### 9.1.1 Jito client (`beast-exec::jito`)

- JSON-RPC against the Block Engine: `sendBundle`, `getBundleStatuses`, `getInflightBundleStatuses`, `getTipAccounts`; tip-floor REST (already used in the repo) and 🔎 the tip-stream WebSocket if available.
- **Do not reuse `TIP_ACCOUNTS` from `helius_sender.rs`** for Jito bundles — those look like Helius Sender's own list. Fetch Jito's via `getTipAccounts` at startup and refresh periodically; pick tip account randomly per bundle.
- Multi-region: Amsterdam, Frankfurt, London, New York, Salt Lake City, Singapore, Tokyo (🔎 current list). Send the same bundle to the best 2–3 regions concurrently (identical bundles dedupe by hash); measure per-region landing rate.
- Rate limits: default per-IP limits are low (historically ~1 request/s per region) → token-bucket per region; 🔎 request an auth UUID if limits bind; handle 429 with jittered backoff.
- Single-tx fallback: Jito's transaction endpoint with `bundleOnly=true` (🔎 verify) wraps a single tx as a bundle so it still gets revert protection.
- **BAM adapter** (🔎): Jito's Block Assembly Marketplace reached roughly a third of stake by Q2 2026. Keep `Submitter` as a trait (`JitoSubmitter`, later `BamSubmitter`) so BAM-specific plugin/sequencing semantics can be added without touching strategies.

#### 9.1.2 Transaction builder

- **Versioned transactions (v0)** with Address Lookup Tables.
- Instruction ordering template for flash strategies:

```
 0  ComputeBudget: SetComputeUnitLimit (from simulation × 1.10)
 1  ComputeBudget: SetComputeUnitPrice (minimal; bundle tip handles priority)
 2  [optional] ATA create-idempotent for any new mint (rent paid by wallet; refundable)
 3  Flash provider: START / BORROW
 4…n  Strategy legs (swaps / liquidation / etc.)
 n+1  Flash provider: REPAY / END
 n+2  Guard: AssertTokenBalanceGte(profit_account, start + tip + fees + min_profit)
 n+3  Jito tip transfer (SystemProgram::Transfer to tip account)   # included in same tx
```

- Notes: many flash providers require borrow/repay as **top-level** instructions (not via CPI) and verify instruction indices/sysvar instructions — 🔎 verify per provider; this shapes how the guard program is allowed to be called (guard is a separate top-level instruction, which is fine).
- **Size budget:** transaction limit is 1,232 bytes. Implement `estimate_size(ixs, alts)` and abort/replan before signing. Constrain Jupiter with `maxAccounts` and `restrictIntermediateTokens`; prefer direct two-pool routes built locally over aggregator multi-hop routes when size is tight.
- **Account-lock budget:** 🔎 per-tx account limit (historically 64); count writable accounts for contention.

#### 9.1.3 Address Lookup Table manager

- Maintains several ALTs by domain: `core` (programs, sysvars, tip accounts, wSOL/USDC/USDT mints, ATAs), per-DEX (pool/vault/oracle accounts for the top pools), per-lender (reserves, banks, oracles).
- Lifecycle: create → extend in chunks (≈ 20–30 addresses/tx) → wait one slot for activation → use. Deactivate + close later to recover rent (≈ 512-slot cooldown 🔎).
- Merges its own ALTs with ALTs returned by Jupiter's `swap-instructions` response.
- Metrics: ALT hit rate (addresses resolved via ALT vs inline), tx size saved.

#### 9.1.4 Compute-unit tuner

- Simulate → read `unitsConsumed` → set limit = `ceil(consumed × 1.10) + 5,000`.
- Cache per-route CU history (p95) to avoid simulating cold.
- 🔎 Consider `SetLoadedAccountsDataSizeLimit` to reduce loaded-data cost where supported.

#### 9.1.5 Profit-guard (two options)

**Option A — Guard-lite (no deployment, start here).** Use the final swap leg's minimum-out as the guard: set `otherAmountThreshold` (min out) on the last leg to `borrow + flash_fee + tip + fees + min_profit` (🔎 verify that editing the quote's threshold before calling `swap-instructions` is honored; otherwise use Option B). For legs built locally (own DEX instruction builders), set min-out natively.

**Option B — On-chain guard program (`programs/guard`).** A tiny `pinocchio` (no_std) program, ~3–6 KB:

| Ix | Accounts | Data | Check |
|---|---|---|---|
| `0 AssertTokenGte` | `[token_account]` | `min_amount: u64` | `token_account.amount >= min_amount` else error `ProfitTooLow` |
| `1 AssertLamportsGte` | `[wallet]` | `min_lamports: u64` | `wallet.lamports >= min_lamports` |
| `2 AssertTokenDeltaGte` (optional) | `[token_account, snapshot_pda]` | `min_delta: u64` | uses a snapshot written earlier in the same tx |

Why a single absolute assertion is safe: only the bot touches the profit account, and `min_amount = observed_balance + required_delta` is computed from state at build time. To avoid races between in-flight bundles, keep **one profit account per strategy** and an **in-flight lock** per profit account in the submitter.

Deployment notes: need a temporary buffer + program-data rent (≈ 0.04–0.15 SOL depending on size; buffer rent is refunded after deploy); set upgrade authority to a cold key or burn after audit; fuzz with Trident; unit-test with Mollusk.

**Accounting rule (SOL-denominated first):** start and end legs in wSOL so the guard unit equals the tip unit (lamports). For token-denominated profit (e.g. USDC), convert `tip + fees` at the current quote and add a safety margin; this is why SOL-start strategies ship first.

#### 9.1.6 Simulator pipeline

1. **Local model** (µs): drop if modeled net < threshold.
2. **`simulateTransaction`** with `replaceRecentBlockhash=true`, `sigVerify=false`, returning pre/post accounts for the profit account → compare simulated delta to modeled delta (`model_error_bps` metric); drop if error > tolerance.
3. **`simulateBundle`** (via a bundle-aware RPC 🔎) for multi-tx bundles; otherwise simulate each tx sequentially against forked state.
4. Re-check blockhash validity and leader window immediately before send.

#### 9.1.7 Submission policy

- Send only if one of the next `k` leaders (default 2) is Jito/BAM-enabled; otherwise skip (no Jito tip would help).
- Opportunity TTL in slots; if the slot passes, **rebuild from fresh state**, never resend stale bundles.
- Poll `getInflightBundleStatuses` (recent window) then `getBundleStatuses` for finality; map to `landed / dropped / invalid / expired`.
- Per-strategy rate limits and cooldowns; "same-opportunity" deduplication key (pool pair + direction + size bucket).

#### 9.1.8 Settlement tracker and P&L ledger

- On `landed`, fetch the tx (`getTransaction`) → parse pre/post balances → compute **realized** gross, tip, fees, net; compare with modeled; store in `settlements`.
- P&L ledger in lamports **and** USD at the time of settlement (for tax export, §26).
- Reconciliation job: wallet balance now = starting balance + Σ(net) − rent movements; alerts on mismatch.

### 9.2 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P2-1 | Jito client (HTTP, regions, rate limits, status polling) | L | P0 |
| P2-2 | Tip account discovery + tip floor/stream integration | S | P2-1 |
| P2-3 | Tx builder with v0 + ALT resolution + size estimator | L | P0 |
| P2-4 | ALT manager (create/extend/close, domains, metrics) | M | P2-3 |
| P2-5 | CU tuner + simulation cache | M | P2-3 |
| P2-6 | Guard-lite (min-out) implementation + tests | S | P2-3 |
| P2-7 | Guard program (pinocchio) + Mollusk tests + Trident fuzz | L | P0-10 |
| P2-8 | Simulator pipeline (local → RPC → bundle) with model-error metric | M | P1-5 |
| P2-9 | Submitter trait, Jito impl, leader-window gating, TTL/dedupe | M | P2-1, P1-14 |
| P2-10 | Settlement tracker + P&L ledger | M | P9-1 |
| P2-11 | Canary controller (per-strategy micro-limits, ε-greedy tip exploration) | M | P2-9, P7 |
| P2-12 | `beast canary --dry-submit` (full pipeline, no send) and `--live` (micro limits) | S | P2-11 |
| P2-13 | Chaos hooks (inject RPC failures, stale blockhash, rate-limit 429s) | M | P2-9 |

### 9.3 ✅ DoD (G2)

- ≥ 20 Canary bundles built; every **dropped** bundle verified to cost 0 lamports (wallet balance reconciliation).
- ≥ 1 landed bundle where realized net equals modeled net within ±5%.
- Intentionally unprofitable bundle (guard threshold raised) is rejected **on-chain** (simulation shows `ProfitTooLow`) and never lands.
- Pipeline p95 latency (state update → bundle sent) documented per tier.

### 9.4 Risks

- ⚠️ **Mis-specified guard** is the single most dangerous bug in the project (it can turn "green-or-revert" into "red-and-land"). Require two independent checks: guard instruction + post-simulation profit assertion, and unit/fuzz tests for the guard before first live bundle.
- ⚠️ Bundle semantics may change with BAM adoption/Alpenglow → isolate in `Submitter`, track Jito release notes (runbook item).
- ⚠️ Rate limits throttle Canary iteration → use multiple regions and conservative cadence.

---

## 10. Phase 3 — Flash-loan layer (Weeks 8–12)

**Goal:** a uniform, provider-agnostic way to borrow any supported mint inside a single transaction and repay it in the same transaction.

### 10.1 Provider abstraction

```rust
pub struct FlashRequest {
    pub mint: Pubkey,
    pub amount: u64,
    pub borrower: Pubkey,          // hot wallet
    pub dest_token_account: Pubkey,
}

pub struct FlashWrap {
    pub start: Vec<Instruction>,   // borrow / start
    pub end: Vec<Instruction>,     // repay / end
    pub extra_alts: Vec<Pubkey>,   // provider ALTs/accounts worth including
    pub est_cu: u32,
    pub fee_lamports_equiv: u64,
}

#[async_trait]
pub trait FlashProvider: Send + Sync {
    fn name(&self) -> &'static str;
    fn supports(&self, mint: &Pubkey, cache: &StateCache) -> bool;
    fn max_borrow(&self, mint: &Pubkey, cache: &StateCache) -> Option<u64>;  // available liquidity, minus safety margin
    fn fee(&self, mint: &Pubkey, amount: u64, cache: &StateCache) -> Option<u64>;
    fn wrap(&self, req: &FlashRequest, cache: &StateCache) -> Result<FlashWrap>;
}
```

### 10.2 Providers

| Provider | Mechanism (🔎 verify against current IDL/docs) | Fee | Notes / quirks |
|---|---|---|---|
| **Kamino Lend** | `flash_borrow_reserve_liquidity` … `flash_repay_reserve_liquidity(borrow_instruction_index)` validated via the instructions sysvar | Per-reserve config; typically very small | Requires reserve refresh state; borrow/repay normally must be top-level (no CPI); large stable/SOL liquidity |
| **MarginFi** | `lending_account_start_flashloan` → borrow/repay inside → `lending_account_end_flashloan` (health checked only at end) | 🔎 reported none or minimal | Needs a MarginFi account for the borrower (rent); oracle accounts as remaining accounts → many accounts, ALTs mandatory |
| **Jupiter Lend** | Flash-loan module (Fluid-style architecture) with borrow/payback instructions | 🔎 reported very low or zero | By deposits it is now the largest Solana lender (Oct 2026 report) → deepest liquidity for flash loans; verify instruction layout and CPI rules |
| **Save (ex-Solend)** | `FlashBorrowReserveLiquidity` / `FlashRepayReserveLiquidity` with borrow-ix index | 🔎 historically ~0.3% — usually the most expensive | Useful for long-tail assets others don't list |

### 10.3 Provider selection algorithm

For a plan needing `(mint, amount)`:
1. Filter providers where `supports` and `max_borrow ≥ amount × 1.02`.
2. Score = `fee_lamports_equiv + λ_size × extra_tx_bytes + λ_cu × est_cu`.
3. Pick the lowest score; keep a ranked fallback list; re-plan if tx size is exceeded.
4. **Never flash-borrow from the same lender you are liquidating** unless explicitly validated (liquidity/health interplay and circular dependencies).
5. Nested flash loans from two providers (to exceed single-provider liquidity) are allowed only behind a feature flag (tx size!).

### 10.4 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P3-1 | Decoders for lender state (reserves/banks/vaults) with liquidity + fee extraction | L | P1-3 |
| P3-2 | `KaminoFlash` (IDL-driven builder via `idl.rs`) | M | P3-1 |
| P3-3 | `MarginfiFlash` incl. account bootstrap (create marginfi account once) | M | P3-1 |
| P3-4 | `JupiterLendFlash` | M | P3-1 |
| P3-5 | `SaveFlash` | S | P3-1 |
| P3-6 | Provider selector + unit tests with fixtures | S | P3-2..5 |
| P3-7 | Fork tests (Surfpool/LiteSVM): borrow→noop→repay succeeds; borrow without repay **fails**; repay short by 1 lamport **fails** | M | P0-10 |
| P3-8 | Provider health monitor (liquidity, paused flags, fee changes) → alerts | S | P3-1 |

### 10.5 ✅ DoD

- All four providers build correct start/end instructions verified in fork tests, including negative tests.
- `beast flash-check --mint <MINT> --amount <N>` prints chosen provider, fee, size/CU estimate.
- Provider configuration is data-driven (program IDs and fee sources come from config + on-chain state, not hardcoded constants).

---

## 11. Phase 4 — Flash-arbitrage strategies (Weeks 10–16)

### 11.1 Common pipeline for every arb strategy

1. **Trigger:** a pool/vault/oracle account update from the feed.
2. **Candidate lookup:** pools indexed by unordered mint pair + LST/stable groups.
3. **Spread test:** marginal price difference after fees > threshold.
4. **Sizing:** closed form where possible (§11.2), otherwise bounded search.
5. **Plan:** borrow plan (provider selection), legs, estimated size/CU.
6. **Cost model + risk gates** (§5.1, §14).
7. **Build → simulate → submit** (P2).
8. **Settle → learn** (tip shading, cooldowns, pool blacklist on repeated model errors).

### 11.2 S1 — Cross-DEX two-leg arbitrage (start here)

**Thesis:** the same pair is quoted in two pools with different reserves/fees; buy on the cheaper, sell on the dearer, in one transaction, funded by a flash loan.

**Optimal size (two constant-product pools).** Let quote token = X (e.g. SOL), base = Y. Pool 1 has reserves `(x1, y1)` and fee-kept factor `γ1 = 1 − fee1`; pool 2 `(x2, y2)`, `γ2`. Buy Y on pool 1 with `a` of X, sell all Y on pool 2:

```
out(a) = K·a / (M + N·a)
K = γ1·γ2·x2·y1
M = x1·y2
N = γ1·(y2 + γ2·y1)
profit(a) = out(a) − a
optimal a* = ( sqrt(K·M) − M ) / N         # exists iff K > M  (spread exceeds fees)
profit*   = out(a*) − a*
```
Cap `a*` by flash liquidity, per-pool price-impact limit, and tx-size/CU limits. For CLMM/DLMM pools use golden-section search on the piecewise-concave profit curve (walk ticks/bins in the local model).

**Data:** reserves via vault token-account updates (CPMM/AMM v4), `sqrt_price/liquidity/tick arrays` (Orca/Raydium CLMM), `active_id/bins` (Meteora DLMM).

**Tx shape:** `[CU] [flash borrow X] [swap pool1: X→Y] [swap pool2: Y→X] [flash repay] [guard] [tip]`.

**Competition and edge:** head-on SOL/USDC mega-pools are the most contested. Edge candidates: long-tail pairs, pairs with ≥3 venues, newly-listed LSTs, pools with unusual fee tiers, and moments after large swaps that other bots ignore due to small size.

**Failure modes:** stale reserves (state lag), fee-tier misread, tick-array not loaded, token extension surprises, CU overrun, tx too large.

**KPIs:** detection→send latency, model error (bps), landing rate by tip-ratio, net SOL/day, blacklist hit rate.

**Config sketch:**
```toml
[strategies.cross_dex_arb]
mode = "radar"                # off | radar | canary | live
start_mints = ["So11111111111111111111111111111111111111112"]   # wSOL
min_net_lamports = 20000
max_borrow_lamports = 50_000_000_000   # 50 SOL equivalent cap
max_price_impact_bps = 80
tip_alpha_initial = 0.5
tip_cap_lamports = 2_000_000
venues = ["raydium_cpmm","raydium_amm_v4","raydium_clmm","orca_whirlpool","meteora_dlmm","meteora_damm","pumpswap","phoenix"]
pair_allowlist = []           # empty = auto-select by P1-4
pair_denylist = []
```

### 11.3 S2 — Triangular and multi-hop arbitrage

**Thesis:** negative cycles in the token graph (`A→B→C→A`) where the product of effective rates exceeds 1.

- Graph edges use *effective* rates at a probe size (e.g. 1 SOL equivalent) after fees; weights `−ln(rate)`; cycle search with Bellman-Ford/SPFA **limited to 2–3 hops** (tx size), restricted to start tokens SOL/USDC/USDT.
- Incremental: on a pool update, only re-evaluate cycles through that edge.
- After a candidate is found, size it with a bounded search using exact local math along the path.
- Use Jupiter `quote` only as a **verification oracle** (and for venues not modeled locally), with `maxAccounts` and `restrictIntermediateTokens` set; prefer self-hosted `jupiter-swap-api` to avoid public rate limits and latency. 🔎

**Risks:** state staleness grows with hop count; tx size explodes → hard cap at 3 hops initially.

### 11.4 S3 — LST / stake-pool arbitrage

**Thesis:** LSTs (jitoSOL, mSOL, bSOL, JupSOL, others) have a protocol exchange rate to SOL; DEX prices deviate from it.

Two atomic directions:

| Direction | Path | When profitable | Atomic? |
|---|---|---|---|
| **A. Mint-and-sell** | flash-borrow SOL → deposit SOL into the stake pool (receive LST at pool rate, minus deposit fee) → sell LST on DEX → repay | DEX price of LST > pool rate × (1 + fees) | ✅ |
| **B. Buy-and-instant-exit** | flash-borrow SOL → buy LST on DEX → exit to SOL via an *instant* path (Sanctum Infinity pool or Marinade liquid-unstake, each with a fee) → repay | DEX price of LST < instant-exit value × (1 − fees) | ✅ |
| **C. Delayed unstake** | buy LST cheap → unstake → wait for epoch | n/a | ❌ not atomic → **excluded** |

- Pool rate: SPL stake pool = `total_lamports / pool_token_supply`; Marinade via its state price; Sanctum via its SOL-value calculator programs 🔎.
- Read pool-level fees (deposit/withdraw) from state; some pools have caps/limits → treat as constraints in sizing.
- **Epoch-boundary research item (R-S3-1):** pool rates step up as epoch rewards are accounted (`UpdateStakePoolBalance` is a permissionless crank). Measure in Radar whether DEX prices lag the new rate around epoch boundaries and whether the crank itself can be bundled profitably. Expected edge: small; value is mostly *information*.
- Output: ranked LST pairs by historical spread, size capacity, and exit-fee profile.

### 11.5 S4 — Stablecoin arbitrage

**Thesis:** USDC/USDT/PYUSD/USDS-class pairs trade in stable-curve and concentrated pools; imbalances create sub-bps to few-bps spreads that scale with size. Flash loans make large sizes possible with no capital.

- Math: stable-swap invariant (for stable pools), tight-range CLMM walk for concentrated pools.
- Profit unit is the stablecoin, so the guard must convert `tip + fees` from SOL to the stable unit with a safety margin (see §9.1.5 accounting rule). Ship after S1/S3 prove the SOL-unit pipeline.
- Depeg events: only *on-chain atomic* paths count; mint/redeem with issuers is not atomic and is excluded.
- Watch for pools with freeze/blacklist behaviors (issuer-level restrictions) → token safety filter.

### 11.6 S5 — New-mint multi-pool arbitrage (pump.fun-aware)

**Thesis:** newly graduated or newly listed tokens often get *more than one* pool (PumpSwap plus third-party Raydium/Meteora/Orca pools), with large temporary price gaps.

- **Precision note:** a pump.fun bonding curve closes at migration and the PumpSwap pool inherits its end state; so there is no "curve vs AMM" gap after graduation. The edge is **between pools** for the same mint once other pools appear. Re-verify migration mechanics before building 🔎.
- Reuse existing code: `ws.rs` / `pumpportal.rs` for new-mint discovery, `idl/pumpfunamm.json` for PumpSwap instruction building. PumpSwap fee parameters (LP/protocol/creator) are configured on-chain → read them from the config account rather than hardcoding 🔎.
- **Safety filter is mandatory** (§11.8). Because the tx is atomic and revert-protected, rug risk to the wallet is limited to dropped opportunities — but hostile token behavior (transfer hooks, CU bombs, fee-on-transfer) can still break plans and waste the compute budget.
- Run only with strict `max_borrow` caps and short TTLs; new pools move fast.

### 11.7 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P4-1 | S1 detector + closed-form sizing + CLMM/DLMM search | L | P1-5, P1-7 |
| P4-2 | S1 builder (local swap ix builders for each venue) | XL | P2-3 |
| P4-3 | Venue builders ordering: start with Raydium CPMM + PumpSwap + Orca, then Meteora DLMM, Raydium CLMM/AMM v4 | (incl. P4-2) | — |
| P4-4 | Jupiter integration (`quote`, `swap-instructions`, self-host option) as verifier / fallback leg builder | M | P2-3 |
| P4-5 | S2 graph cycle detector + incremental updates | L | P1-6 |
| P4-6 | S3 stake-pool state decoders + instruction builders (SPL stake pool, Marinade, Sanctum) | L | P3-1 |
| P4-7 | S4 stable-curve math + guard unit conversion | M | P4-1 |
| P4-8 | S5 new-mint multi-pool detector + safety filter integration | M | P1-4 |
| P4-9 | Per-strategy tip shading + cooldowns + pool blacklist on model-error | M | P2-11 |
| P4-10 | Canary programs (per strategy) with micro-limits and daily cost cap | S | P7 |
| P4-11 | Backtests on recorded feeds (replay) with synthetic "competitor" model from Radar labels | M | P1-10 |

### 11.8 Token safety and Token-2022 screening (cross-cutting)

Reject or flag a mint when:

| Check | Why |
|---|---|
| Freeze authority present (unless allowlisted stable issuers) | account can be frozen mid-plan |
| Mint authority present on a non-blue-chip | supply can be inflated |
| Token-2022 `TransferFee` | output ≠ input; models must include it (or skip) |
| Token-2022 `TransferHook` | unknown CPI, CU and account surprises |
| `PermanentDelegate`, `NonTransferable`, `ConfidentialTransfer` | blocks or confiscates |
| Pool with < min TVL or < min age | manipulable / unstable |
| Simulation shows CU spike vs model | possible CU bomb |
| Repeated model-error for pool | blacklist with TTL |

### 11.9 ✅ DoD (G3)

- S1 Radar→Canary→Live promotion path implemented and documented.
- 14-day Canary/Live window: realized net P&L > 0 after all tips/fees; dropped-bundle cost = 0; no guard violations.
- Model error p95 < 25 bps on landed bundles.
- A strategy post-mortem template filled for the first 10 landed or lost-race cases.

### 11.10 Risks

- ⚠️ Prop AMMs (oracle-priced, closed-source) dominate large flows; they cannot be modeled locally → use Jupiter quotes/simulation as the only information source and expect lower hit rates.
- ⚠️ Venue instruction builders are the largest source of bugs → golden tests per venue from real mainnet transactions (decode → rebuild → byte-compare).
- ⚠️ Tx size/CU limits silently kill otherwise good opportunities → size/CU telemetry per rejected plan.

---

## 12. Phase 5 — Liquidation engine (Weeks 14–20)

### 12.1 Why liquidations fit the constraints

Liquidating an unhealthy position requires repaying part of the borrower's debt and returns discounted collateral. With a flash loan to fund the repayment and a swap of the seized collateral back into the debt mint, the entire sequence is atomic and capital-free, and unprofitable attempts revert.

> 🔎 Market context (Oct 2026): Jupiter Lend overtook Kamino as the largest Solana lender by deposits, and Kamino cut liquidation penalties by roughly 90% in response to competition. Per-liquidation margins are therefore smaller than in earlier years; the strategy depends on **volume, long-tail markets, and speed**, not fat bonuses.

### 12.2 Architecture

```
 Lender program accounts (gRPC, filtered by owner + discriminator + size)
 Oracle accounts (Pyth push/pull receivers, Switchboard)  ───────────────┐
                │                                                        │
                ▼                                                        ▼
        ObligationIndex (per protocol)  ──►  HealthEngine (recompute on price/interest tick)
                │                                      │
                ├── Watchlist (distance-to-liquidation heap)             ▼
                └── Triggered set (health < 1)  ──►  LiquidationPlanner ──► Opportunity
```

- **Discovery:** initial snapshot with paginated `getProgramAccounts` (use provider-specific paged variants to avoid timeouts), then incremental gRPC account updates.
- **HealthEngine:** recompute only obligations affected by a changed reserve/bank/oracle (reverse index: `reserve → obligations`, `oracle → reserves`).
- **Watchlist:** maintain for each obligation the *liquidation price* of its dominant collateral given other prices; keep a min-heap by relative distance. On an oracle tick, only the heap head region needs checking.
- **Pre-warming:** for the top K watchlist entries, pre-build the invariant parts of the transaction (refresh instructions, ATAs, ALT contents) so the trigger path only fills amounts and signs.
- **Interest accrual:** health also drifts with accrued interest; schedule periodic recomputes (e.g. every N slots) for near-threshold positions.

### 12.3 Generic health math

```
collateral_weighted = Σ_i  deposit_i × price_i × liquidation_threshold_i
debt_weighted       = Σ_j  borrow_j  × price_j × borrow_factor_j
unhealthy           ⇔  debt_weighted  >  collateral_weighted     (protocol-specific exact form)
```
Use protocol-specific details from IDLs: confidence-interval handling (lower bound for collateral, upper bound for debt), EMA vs spot, elevation/e-mode groups, isolated assets, caps.

### 12.4 Profit model

```
repay_value      = chosen_repay_amount × debt_price
bonus_value      = repay_value × liquidation_bonus_bps / 10_000
seized_value     = repay_value + bonus_value            (capped by available collateral)
swap_loss        = seized_value − out_value_after_swap  (slippage + DEX fees)
flash_fee, tip, fees as in §5.1
net              = bonus_value − swap_loss − flash_fee − tip − fees
```
Choose `chosen_repay_amount` to maximize `net` subject to: close-factor cap, minimum liquidation value, flash liquidity, collateral availability, tx size/CU, and swap depth. Skip positions where collateral < debt (bad debt: negative bonus).

### 12.5 Transaction shape

```
 [CU limit] [CU price]
 [ATA create-idempotent: debt mint, collateral mint]            # rent refundable
 [lender: refresh_reserve × N ; refresh_obligation]              # protocol-specific freshness
 [flash BORROW debt mint from provider ≠ target lender]
 [lender: liquidate_*  (repay debt, receive collateral)]
 [swap collateral → debt mint (local venue builder or Jupiter swap-instructions)]
 [flash REPAY]
 [guard: profit ≥ start + tip + fees + min_profit]
 [tip]
```

### 12.6 Protocol playbooks

#### 12.6.1 Kamino Lend
- Instruction family: `liquidate_obligation_and_redeem_reserve_collateral` (plus refresh instructions; 🔎 exact names/order from IDL).
- Multiple lending markets (main, JLP-focused, altcoin, LST-focused, …) → index each market; elevation groups change thresholds.
- Liquidation bonus scales between a min and max by how far LTV exceeds the threshold; close factor and min-value rules are per reserve/market config.
- Observed trend: reduced penalties → require lower `min_net_lamports` but higher volume tolerance.

#### 12.6.2 Jupiter Lend
- Fluid-style vault architecture with **tick-based** positions; liquidation absorbs debt from ticks in bulk rather than per-obligation. 🔎 Study docs/IDL first: instruction names, how the liquidator is paid, and whether liquidation requires swaps.
- Index vaults/ticks rather than obligations; monitor tick distribution vs oracle price to predict liquidation events.
- Flash-loan Multiply vaults indicate heavy leverage → liquidation cascades possible; monitor concentration.

#### 12.6.3 MarginFi
- Liquidator needs its own MarginFi account (one-time rent). With `start_flashloan`/`end_flashloan`, health is checked only at the end.
- Banks, oracle accounts passed as remaining accounts → account lists are long; ALTs mandatory.
- 🔎 Historically a liquidator bonus plus an insurance-fund cut; verify current parameters. Oracle pricing on LST collateral is a known weak point → be conservative with stale/low-confidence oracles.

#### 12.6.4 Save (ex-Solend)
- Isolated pools and long-tail assets; per-reserve liquidation bonus and close factor. 
- `LiquidateObligationAndRedeemReserveCollateral` + refresh sequence.
- Lower competition on obscure markets, but also lower collateral liquidity → swap loss dominates; require stricter depth checks.

#### 12.6.5 Not in scope
- Venues requiring the liquidator to hold margin/collateral (e.g. perp-DEX liquidations that need a funded margin account) → out of scope under C1. Revisit only if the owner changes constraints.

### 12.7 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P5-1 | Obligation/vault decoders + index (Kamino first) | L | P1-3 |
| P5-2 | HealthEngine (+ reverse indices, oracle ticks) with fixtures from real liquidations | L | P5-1 |
| P5-3 | Watchlist heap + pre-warm cache | M | P5-2 |
| P5-4 | Kamino liquidation builder + refresh sequencing | L | P3-2 |
| P5-5 | Swap-back leg (local venue builders + Jupiter fallback) | M | P4-2, P4-4 |
| P5-6 | Repay-amount optimizer (close factor, depth, tx limits) | M | P5-4 |
| P5-7 | MarginFi decoders + liquidator account bootstrap + builder | L | P3-3 |
| P5-8 | Jupiter Lend vault/tick decoders + liquidation builder | XL | P3-4 |
| P5-9 | Save decoders + builder | M | P3-5 |
| P5-10 | Historical replay: re-simulate known liquidations to measure "could we have won" | M | P1-10 |
| P5-11 | Bad-debt and oracle-staleness guards | S | P5-2 |
| P5-12 | Liquidation dashboard (health bars, watchlist, outcomes) | M | P8 |

### 12.8 ✅ DoD (G4)

- Kamino liquidator runs in Radar for 14 days; replay shows the share of historical liquidations we *could* have won given measured latencies.
- ≥ 1 landed liquidation or a data-backed stop/continue decision.
- Fork tests: liquidation of a synthetic unhealthy obligation succeeds; healthy obligation attempt reverts; insolvent obligation is skipped.

### 12.9 Risks

- ⚠️ Liquidation races are decided in sub-slot time; without Tier 2 infra, expect to win mainly in long-tail markets.
- ⚠️ Oracle quirks (staleness, confidence, EMA) cause false positives → require simulation success before send (already enforced by pipeline).
- ⚠️ Protocol upgrades change IDLs → IDL hash monitoring + CI test against fresh on-chain IDLs.

---

## 13. Phase 6 — Streaming-dependent strategies (Weeks 18–24)

These strategies need low-latency data and usually **infra Tier 2** (§5.4). They are sequenced last on purpose: the earlier phases produce the data that tells you whether Tier 2 is worth buying.

### 13.1 Backrun engine

**Reality check:** Solana has no public mempool (Jito's public mempool streams were shut down in 2024). You cannot attach your transaction to someone else's pending transaction inside a bundle. Backrunning therefore means: *observe what's about to land or has just landed, predict the resulting state, and race to the residual spread.* If the spread is gone by execution time, the guard fails and the bundle is dropped (no cost).

**Information sources (increasing speed and cost):**

| Source | Latency | Notes |
|---|---|---|
| Yellowstone gRPC at `processed` commitment | after execution | simplest; slot+0/+1 backrun |
| Raw **shred** stream (e.g. Jito ShredStream proxy or provider equivalents) | before execution/`processed` | decode entries into txs, simulate effects on a speculative overlay; 🔎 access requirements and regions |
| Co-located validator/RPC node | lowest | Tier 3, not recommended |

**Components**

1. **Speculative state overlay:** a copy-on-write layer over `StateCache` where decoded-but-unexecuted transactions' *predicted* effects (swap deltas on known pool programs) are applied; roll back when the real account updates arrive.
2. **Trigger classifier:** which tx types can create opportunities — large swaps on modeled pools, oracle price-update posts, liquidations (collateral dumps), LST rate updates, big limit-order fills, new-pool creation.
3. **Residual-spread evaluator:** run the S1/S2/S3 detectors against the overlay instead of the base cache.
4. **Fire policy:** send the bundle immediately with a tip tied to `p_land(α)` learned in Canary; only if a Jito/BAM leader is within the next `k` slots.
5. **Dedupe/cooldown:** one attempt per (pool-set, direction) per slot.

**BAM note:** BAM changes how transactions are ordered and what searchers can observe (TEE-based sequencing and plugins). Treat backrun semantics as *subject to change*; keep detectors independent of submission mechanics and keep a `BamSubmitter` stub ready. 🔎 Track adoption (about a third of stake by Q2 2026) and plugin APIs.

**Alpenglow/Firedancer note:** finality and block-production timing are changing in 2026 🔎. Re-measure latency budgets after each client/consensus upgrade.

### 13.2 Pyth Express Relay integration

**Idea:** instead of racing everyone to land a liquidation (or order fill), protocols integrating Express Relay publish the opportunity to registered searchers who **bid** for the right to execute it. Opportunities are handed to you; the edge is pricing and liquidity, not raw latency — an excellent fit for a gas-only searcher.

- 🔎 Verify which Solana protocols currently integrate it and on which flows (liquidations, limit orders), the registration/permission model, the bid format and fee split.
- `ExpressRelayClient`: subscribe to opportunities (WebSocket), build the execution tx (flash-funded as usual), simulate, submit a bid, track outcome.
- Bidding policy: bid = `β × expected_net`, where β is learned from win/loss feedback; same guard and same profit accounting as bundles.
- Fallback: if the integration is gated or inactive, mark the strategy `Off` and keep the code behind a feature flag.

### 13.3 Oracle-lag and "price-update + liquidate" bundles

Two legitimate patterns around pull-model oracles:

1. **Oracle-lag arbitrage:** compare off-chain signed prices (Pyth Hermes) to on-chain prices and to pools whose quotes follow the oracle; if the oracle-priced venue is stale relative to the market, take the spread atomically.
2. **Post-then-act:** for protocols reading Pyth *pull* price-update accounts, the update post is a permissionless instruction that anyone may submit with a **genuine signed price update**. Bundle `[post_price_update] + [liquidate]` so the position becomes liquidatable and is liquidated in the same bundle.

> ⚠️ Only ever post **authentic, unmodified signed updates** from the oracle network. Anything resembling price manipulation is out of scope and prohibited by §1.2.

### 13.4 Opportunity cascades (research)

Liquidations dump collateral into pools; large arbs shift LST/stable pools; both create secondary dislocations. Radar can label "child" opportunities by parent event, quantifying how often one liquidation produces a profitable backrun. If strong, build a cascade planner (single bundle: liquidation + resulting arb).

### 13.5 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P6-1 | ShredStream/gRPC tx decoder (known-program decoders → `SwapDelta`) | L | P1-2 |
| P6-2 | Speculative overlay + rollback correctness tests | L | P1-3 |
| P6-3 | Trigger classifier + residual-spread evaluator | M | P4-1 |
| P6-4 | Fire policy + dedupe/cooldowns + region fan-out | M | P2-9 |
| P6-5 | BamSubmitter stub + feature flag | S | P2-9 |
| P6-6 | ExpressRelayClient + bidding policy | L | P2-9 |
| P6-7 | Hermes/Pyth price-update decoder + oracle-lag detector | M | P1-2 |
| P6-8 | Post-then-act bundle builder (Pyth receiver + lender) | L | P5-4 |
| P6-9 | Cascade labeling in Radar | M | P1-9 |

### 13.6 ✅ DoD

- Backrun in Canary for ≥ 7 days with a measured landing rate and net P&L.
- Express Relay: either a working Canary bid flow or a documented "not available/not worth it" ADR.
- Overlay correctness: replay tests show predicted vs actual post-state error within tolerance.

### 13.7 Risks

- ⚠️ Cost of Tier 2 infra can exceed early returns → obey the infra gate.
- ⚠️ Speculative state bugs produce confident-but-wrong plans → rely on simulation + guard (cost: only wasted compute).

---

## 14. Phase 7 — Risk engine and safety (continuous, core by Week 10)

### 14.1 Control catalogue

| Control | Scope | Default | Action when breached |
|---|---|---|---|
| `balance_floor_lamports` | wallet | 0.05 SOL | block all sends; alert |
| `min_net_lamports` | strategy | per strategy | skip opportunity |
| `tip_cap_lamports` | strategy | per strategy | clamp |
| `tip_alpha_max` | strategy | 0.90 | clamp |
| `max_borrow_lamports` | strategy | per strategy | clamp or skip |
| `max_inflight_bundles` | strategy / global | 3 / 6 | queue or skip |
| `max_attempts_per_min` | strategy | per strategy | throttle |
| `daily_landed_cost_cap` | strategy | per strategy | demote to Radar until next UTC day |
| `max_model_error_bps` | strategy | 50 | blacklist pool; if repeated, demote strategy |
| `max_feed_lag_ms` | global | tier-dependent | pause sending |
| `max_slot_lag` | global | 3 | pause sending |
| `rpc_consensus` | global | 2 sources agree on slot ± 2 | pause on disagreement |
| `clock_skew_ms` | global | 250 | pause sending |
| `max_consecutive_failures` | strategy | 25 build/sim failures | demote to Radar |
| `unexpected_balance_delta` | wallet | > 1,000 lamports unexplained | **hard halt** + alert |
| `program_upgrade_detected` | provider/venue | any | pause that venue until IDL re-verified |
| `provider_paused_flag` | lender | any | exclude provider |

### 14.2 Risk state machine

```
 Healthy ──(soft breach)──► Degraded ──(hard breach / manual)──► Halted
    ▲                           │                                   │
    └──────(auto-recover)───────┘                                   │
    ▲                                                               │
    └────────────────(human resume after review)────────────────────┘
```
- **Degraded:** lower rates, Radar-only for the affected strategy, extra logging.
- **Halted:** no sends of any kind; Radar continues; alert with reason and a runbook link.

### 14.3 Promotion and demotion rules

| From → To | Rule |
|---|---|
| Off → Radar | config change only |
| Radar → Canary | gate data met (§23) + manual confirm in dashboard (typed confirmation) |
| Canary → Live | ≥ 14 days Canary, realized P&L > 0, model error p95 < 25 bps, zero guard violations, manual confirm |
| Live → Radar (auto) | realized net below modeled by > tolerance, or daily cost cap hit, or breaker tripped |

### 14.4 Kill switches

1. Dashboard red button (confirm modal).
2. `POST /api/risk/halt` (token auth).
3. Telegram `/kill` (allowlisted chat IDs).
4. Filesystem flag `./STOP` polled each second.
5. Signal handler (`SIGUSR1` = halt, `SIGUSR2` = resume).

### 14.5 Safe configuration changes

Staged apply: `POST /api/config/stage` → validation + diff → `POST /api/config/apply` with a version check. Switching any strategy to Live requires both `--real` at process start **and** an explicit runtime confirmation. Every change is written to an append-only `config_audit` table.

### 14.6 Chaos testing (ties to §19)

Inject: RPC timeouts, stale blockhash, 429 storms, gRPC disconnect/reconnect loops, duplicate account updates, out-of-order slots, malformed accounts, provider pause, oracle staleness. Each scenario has an expected breaker outcome.

### 14.7 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P7-1 | Risk engine core (controls, state machine, evaluator hooks) | M | P1-1 |
| P7-2 | Balance reconciliation + unexplained-delta detector | M | P2-10 |
| P7-3 | Promotion/demotion logic + dashboard confirmations | M | P8 |
| P7-4 | Kill-switch plumbing (API, Telegram, file, signals) | S | P0-8 |
| P7-5 | Staged config + audit trail | M | P0-3 |
| P7-6 | Chaos suite | M | P2-13 |

### 14.8 ✅ DoD

All chaos scenarios produce the expected breaker/halt outcome in CI; a demonstration run shows halt reaching "no sends" within 1 slot of a hard breach.

---

## 15. Phase 8 — Dashboard and control plane (continuous from Week 3)

### 15.1 Information architecture

| Page | Purpose | Key widgets |
|---|---|---|
| **Overview** | state of everything at a glance | wallet balance, today's net P&L, risk state, strategy mode chips, rent recovered, feed health |
| **Strategies** | one card per strategy | Mode toggle (Off/Radar/Canary/Live), KPIs sparkline, funnel (detected→modeled→simulated→sent→landed), tip-α learning curve, blacklist, last 10 outcomes |
| **Radar** | research | live opportunity feed, distribution of modeled net, outcome labels, competitor tip histogram, "what-if win-rate" slider |
| **Opportunity detail** | forensic view | timeline per opportunity, legs, modeled vs simulated vs realized, raw tx links |
| **Bundles** | execution | per-region landing rates, statuses, drop reasons |
| **P&L** | money | realized net by strategy/day, tip efficiency, cost breakdown, ledger table with CSV export |
| **Liquidations** | lenders | health bars (top watchlist), per-protocol counts, recent liquidations, "could-have-won" replay stats |
| **Rent reclaimer** | cleanup | scan results, dry-run diff, run history, recovered total |
| **Infra** | health | latency heatmap per region/provider, feed lag, cache size, CU/size histograms |
| **Risk & controls** | safety | control values vs current usage, breaker state, kill switch, promotion checklist |
| **Config** | settings | schema-driven editor, validation errors inline, staged diff, apply/rollback |
| **Logs & alerts** | ops | structured log viewer with filters, alert history |
| **Bounty radar** | off-chain | upcoming bounties/contests scored by fit |
| **Sniper (legacy)** | existing | holdings, trades (kept; clearly marked "uses your SOL") |

### 15.2 UX principles

- Keep the 7 cyberpunk themes; add semantic color tokens (success/warn/danger) that remain legible in every theme.
- **Mobile-first for monitoring** (the owner is often on a phone): compact cards, collapsible sections, large kill-switch target, PWA install, web-push for landed bundles/halts.
- Dangerous actions (Live toggle, resume after halt) require a typed confirmation, never a single tap.
- Everything numeric shows units (lamports/SOL/USD) with a global toggle.
- Realtime via `/ws/events` (backpressure-aware); polling fallback.

### 15.3 Telegram control bot

- Commands: `/status`, `/pnl [1d|7d|30d]`, `/mode <strategy> <off|radar|canary>` (Live requires dashboard), `/kill`, `/resume` (with confirm), `/reclaim dry`, `/radar top`.
- Alerts: landed bundle (with net), breaker tripped, balance floor, feed down > N seconds, provider paused, new high-fit bounty.
- Security: allowlisted chat IDs; no secrets in messages; rate limit.

### 15.4 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P8-1 | WebSocket event stream + typed event schema | M | P0-8 |
| P8-2 | Overview + Strategies pages | M | P8-1 |
| P8-3 | Radar page + opportunity detail | M | P1-9 |
| P8-4 | Bundles + P&L + export | M | P2-10 |
| P8-5 | Liquidations + Rent reclaimer pages | M | P5, P1-11 |
| P8-6 | Infra + Risk pages | M | P7 |
| P8-7 | Schema-driven Config editor with staged apply | L | P7-5 |
| P8-8 | PWA + web push | S | P8-1 |
| P8-9 | Telegram bot | M | P7-4 |
| P8-10 | Docs site updates (user guide per strategy, runbooks) | M | — |
| P8-11 | Theme tokens + accessibility pass | S | — |

### 15.5 ✅ DoD

Every strategy has a complete card; Live toggles require confirmation; mobile layout verified on a phone-sized viewport; Telegram kill command halts sends in an end-to-end test.

---

## 16. Phase 9 — Data, persistence and analytics (Weeks 4–12)

### 16.1 Storage

- **SQLite (WAL)** via `sqlx`; migrations in `beast-store`; one writer task with batching; periodic `VACUUM`/retention jobs.
- **Parquet exports** (daily) for heavy analysis; DuckDB notebooks provided in `docs/analysis/`.
- **Feed recordings** (`.zst`) kept for N days; selected "interesting" windows archived permanently as test fixtures.
- Backups: nightly local snapshot; optional continuous replication (Litestream-style) to object storage 🔎 cost.

### 16.2 Retention defaults

| Table | Hot retention | Archive |
|---|---|---|
| `opportunities` (Radar) | 30 days full detail | daily aggregates forever |
| `bundles`, `settlements` | forever | — |
| `ledger` | forever | exported yearly |
| `feed_samples` | 7 days | selected windows |
| `config_audit` | forever | — |

### 16.3 Analytics queries to ship

- Opportunity funnel by strategy/day; model-error distribution; competitor-tip percentiles by size bucket; `p_land(α)` curves; net P&L per infra dollar; per-pool win/loss; hour-of-day patterns; epoch-boundary patterns.

### 16.4 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P9-1 | Schema + migrations + writer task | M | P0-2 |
| P9-2 | Retention + vacuum + backup jobs | S | P9-1 |
| P9-3 | Parquet export + DuckDB notebooks | S | P9-1 |
| P9-4 | Tax/ledger export (CSV with USD valuation, §26) | S | P2-10 |
| P9-5 | Analytics views/queries pack | S | P9-1 |

### 16.5 ✅ DoD

Restore-from-backup drill passes; replay of a recorded window reproduces the same opportunity list (deterministic); tax export reconciles to wallet balance changes.

---

## 17. Phase 10 — Operations, infrastructure, deployment (Weeks 8–28)

### 17.1 Environments

| Env | Purpose | Notes |
|---|---|---|
| `dev` | laptop | Radar on free tiers, fork tests |
| `stage` | small VPS | soak tests, Canary |
| `prod` | region-matched host | Tier 1/2; locked-down; only needed when a gate passes |

### 17.2 Host and runtime

- Region: choose by measured latency to Jito block engines and the gRPC provider (use `beast doctor` + latency prober). Common hubs: Frankfurt, Amsterdam, New York, Tokyo. 🔎
- Time sync with `chrony`; NVMe disk; CPU governor `performance`; pin hot threads; `mimalloc`/`jemalloc`; `RUST_LOG` sensible defaults; `ulimit -n` raised; kernel network buffers tuned.
- Process supervision: `systemd` units (`beast.service`, `beast-api.service`), automatic restart with backoff, watchdog.

### 17.3 Observability

- **Prometheus metrics** (examples): `beast_feed_lag_ms`, `beast_slot_lag`, `beast_state_cache_entries`, `beast_opportunities_total{strategy,outcome}`, `beast_model_error_bps`, `beast_sim_latency_ms`, `beast_bundle_sent_total{region}`, `beast_bundle_landed_total`, `beast_bundle_dropped_total{reason}`, `beast_tip_paid_lamports_total`, `beast_net_profit_lamports_total`, `beast_breaker_state`, `beast_rpc_errors_total{provider}`.
- **Grafana** dashboards in `ops/grafana/`; **Alertmanager** → Telegram.
- Logs: JSON → local files with rotation; optional Loki.
- **Tracing:** per-opportunity spans for latency breakdown (feed → detect → build → sim → send).

### 17.4 CI/CD

- Tagged releases build with locked dependencies; artifacts include SBOM and checksums; optionally signed.
- Staged rollout: `stage` soak ≥ 24 h → `prod`.
- Rollback: previous binary retained; DB migrations forward-compatible for one version.

### 17.5 Secrets management

- Keys in files with `0600` or encrypted (age/sops); API tokens via env; no secrets in repo, logs, or dashboard.
- Hot wallet funded with the minimum; sweep job moves profits above a threshold to the cold address.

### 17.6 Runbooks (write each as a one-page doc)

Feed down · RPC provider outage · Jito region degraded · Bundle landing rate collapsed · Balance reconciliation mismatch · Provider paused · Program upgrade detected · Key compromise suspected · Disk full · Clock skew · How to roll back · How to rotate keys · How to add a new venue.

### 17.7 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P10-1 | systemd units, Docker compose (bot + Prometheus + Grafana + Loki) | M | P0-2 |
| P10-2 | Metrics instrumentation across crates | M | P1-1 |
| P10-3 | Grafana dashboards + alert rules | S | P10-2 |
| P10-4 | Release pipeline (build, SBOM, checksums) | S | P0-1 |
| P10-5 | Sweep job (hot → cold) | S | P2-10 |
| P10-6 | Runbooks | M | — |
| P10-7 | 7-day unattended soak test with chaos | M | P7-6 |
| P10-8 | Region selection tooling | S | P1-14 |

### 17.8 ✅ DoD

7-day soak on `stage`: no manual intervention, zero unexplained balance deltas, all alerts delivered, memory stable.

---

## 18. Phase 11 — Off-chain income tracks (parallel, any time)

These require **no capital and no infra** and carry **no transaction risk**; they are the fastest path to income while the on-chain engine matures.

### 18.1 Bounty and contest radar

**Sources to poll** (respect each site's terms and rate limits; prefer official APIs/RSS; 🔎 verify availability):

- Immunefi program listings for Solana projects.
- Superteam Earn listings (bounties, grants, instagrants).
- Audit-contest platforms: Code4rena, Sherlock, Cantina, CodeHawks (Solana/Rust contests).
- GitHub issues labelled `bounty`/`good first issue` across Solana-ecosystem orgs.
- Hackathon calendars (Colosseum and others) and ecosystem grants (Solana Foundation, Jito, others).

**Features:** normalize (reward, currency, deadline, language, scope), score by skill match (Rust/Anchor/Pinocchio/TypeScript), dedupe, notify via Telegram/dashboard, track personal pipeline (interested → working → submitted → paid), calendar export.

### 18.2 Security research lab (for bounties/contests)

- Toolbox: Trident (fuzzing for Anchor programs), Mollusk/LiteSVM/Surfpool (execution/fork testing), `cargo audit`/`cargo geiger`, static analyzers, Anchor/Pinocchio references.
- **Checklist of Solana vulnerability classes** to review programs against: missing signer checks; missing owner/program-id checks; account data matching / type cosplay; unchecked arbitrary CPI; PDA seed collisions and bump handling; re-initialization; closing accounts without zeroing (revival attacks); integer overflow/rounding direction; sysvar/instruction-introspection spoofing; oracle staleness/confidence handling; Token-2022 extension edge cases; upgrade-authority and admin-key risks; rent/lamport accounting errors; CPI privilege escalation; remaining-accounts injection.
- Rules of engagement: stay inside program scope; never test against mainnet funds unless the program explicitly allows it; responsible disclosure; keep PoCs on forks.
- Reuse: SOL BEAST's own fork-test harness doubles as a PoC lab.

### 18.3 Public tools (optional, monetizable, non-custodial)

| Tool | How it earns | Notes |
|---|---|---|
| **Rent reclaimer web app** | visible service fee on reclaimed rent | consent-based; shows every account before signing |
| **Swap frontend / Blink with platform fee** | Jupiter referral/platform-fee mechanism (🔎 verify current program) | transparent fee display |
| **Liquidation-risk checker** | none (growth) or tip jar | paste a wallet → health across lenders, with alert subscriptions |
| **LST rate & spread dashboard** | none (growth/credibility) | reuses S3 data |
| **Jito tip-floor / landing dashboard** | none | reuses telemetry |

### 18.4 Open-source and grants

- Publish `beast-dex` (AMM math/decoders) and the guard program as standalone MIT crates with tests → grant-eligible public goods, and a credibility signal for contract work.
- Write short technical posts from the Edge Memo and post-mortems.

### 18.5 Work items

| ID | Task | Effort | Deps |
|---|---|---|---|
| P11-1 | Bounty radar service + scoring + notifications | M | P0 |
| P11-2 | Pipeline tracker UI | S | P11-1 |
| P11-3 | Research lab docs + templates + PoC scaffolding | S | P0-10 |
| P11-4 | Rent reclaimer web app (non-custodial) | L | P1-11 |
| P11-5 | Referral-fee swap Blink (optional) | M | P4-4 |
| P11-6 | Grant application pack (README, roadmap, metrics) | S | — |

### 18.6 ✅ DoD

First bounty/contest submission or grant application sent; rent web app's fee display audited by a second reviewer.

---
