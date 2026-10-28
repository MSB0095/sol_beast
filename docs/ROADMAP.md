# Roadmap: making SOL BEAST not a placeholder

This is the gap, measured against the repository rather than described. Every
row below is a promise about what will exist after the item is done, and each
item names the check that proves it.

## The gap, measured

| Dimension | Registered | Verified (on-chain bytes) | **Executable** |
|---|---|---|---|
| Swap venues | 60 | 21 (8 modeled + 3 decoded + 2 stake pool + catalogued) | **1** |
| Curve math models | 6 (cpmm, clmm, dlmm, stableswap, weighted, concentrated) | 6 | — |
| Local instruction builders | — | — | **1** (`beast-exec/src/cpmm.rs`) |
| Flash-loan providers | 4 verified | 4 | 4 |
| On-chain profit guard | source shipped | — | **0 deployed** |

`beast-exec` contains exactly one venue builder. That single fact is why a
profitable cycle has never been found: the driver prices two-pool CPMM cycles
only, and every CPMM pair is edge-negative by the two pool fees.

## The promotion ladder (what "not a placeholder" means)

A venue stops being a placeholder only when it reaches the last rung:

1. **Catalogued** — program id and name registered. Currently 6 venues.
2. **Decoded** — account layout verified byte-exact against live accounts.
   Currently 3 venues.
3. **Modeled** — decoder plus quote math verified against a simulation.
   Currently 8 venues.
4. **Executable** — an instruction builder whose transaction a mainnet
   simulation **accepts**. Currently 1 venue (`raydium_cpmm`).

Rungs 2-3 are documentation. Rung 4 is the product. The work below is all rung 4.

## Phase 1 — the seven venues that carry Solana's volume

These hold essentially all Solana swap volume. Each needs a builder module in
`crates/beast-exec/src/` following the shape of `cpmm.rs`: a `Pool` decoder, a
`quote_exact_in` that matches the program's rounding, a `swap_base_input`
instruction builder, and a probe entry point.

| # | Venue | Curve | Why it matters | Known blocker |
|---|---|---|---|---|
| 1 | `raydium_amm_v4` | constant product | 14,788 SOL depth vs CPMM's 2.84 on SOL/USDC | **OpenBook account list.** I verified the pool's `market` (offset 496), `open_orders` (528), `target_orders` (592), `serum_program` (560) and vaults (336/368). The market is a 3228-byte OpenBook **V3** account with embedded orderbook slabs; its vault/signer offsets and the bids/asks/event-queue wiring are unverified. **One real landed swap transaction for a known pool closes this.** Without it I will not guess an account list. |
| 2 | `raydium_clmm` | concentrated | Deep concentrated liquidity; the CLMM tick math already exists in `beast-dex/src/math/clmm.rs` | None known; the layout is marked verified. Needs a builder plus a probe. |
| 3 | `orca_whirlpool` | concentrated | Already `modeled`; `clmm.rs` approximation is in place | Swim with a real builder rather than the current-range approximation, or accept the approximation and prove it against a simulation. |
| 4 | `meteora_dlmm` | dlmm | Already `decoded`; `dlmm.rs` exists | Bin-array walker and a builder. |
| 5 | `meteora_damm_v2` | constant product | Already `modeled` | Builder only. |
| 6 | `pumpswap` | constant product | Already `modeled`, creator-fee aware. The legacy sniper already decodes this venue | Builder; reuse the sniper's fee semantics rather than re-deriving them. |
| 7 | `spl_token_swap` | constant product / stable | Already `modeled`, both curves | Builders for both curve types. |

**Acceptance per venue:** a probe that builds one real swap on mainnet with a
funded wallet, simulates it, and reports `err: none` with the expected output.
Same bar that Kamino, Solend, Marginfi, Jupiter Lend and Raydium CPMM each
cleared.

**Effect:** the driver goes from one venue to seven, so a cycle can mix venues.
A CPMM↔AMM-v4 cycle is the first one that can plausibly pay, because the two
venues disagree about price by more than their fees.

## Phase 2 — prop AMMs (17 registered)

`solfi`, `solfi_v2`, `zerofi`, `goonfi`, `goonfi_v2`, `obric_v2`, `byreal`,
`bisonfi`, `fusion_amm`, `woofi`, `humidifi`, `quantum`, `whale_street`,
`hadronfi_v2`, `lifinity_v1`, `lifinity_v2`, `jupiter_order_engine`.

These cannot be locally quoted: each uses its own swap program with a custom
pricing function, so a generic curve model is wrong by construction. Two
sub-tracks:

* **2a — liveness first.** Several of these venues are dormant or deprecated.
  Check each program's ownership/executability and recent transaction count
  before writing an adapter, and demote the dead ones to `Disabled` with the
  reason recorded. Empty adapters for dead programs are placeholders with extra
  steps.
* **2b — builders for the live ones.** Each needs its own layout work, verified
  against a landed swap, exactly like Phase 1. These are individually the
  hardest items in the list; treat one live prop AMM as a full milestone.

Until 2b lands, keep them `quote_only`: Jupiter routes them and that is honest.

## Phase 3 — order books (3 registered)

`openbook_v2`, `phoenix`, `manifest`. A CLOB is not a curve: executable value
comes from top-of-book plus depth, not a reserve ratio. Each needs a slab
decoder (fifo slab for OpenBook, ring buffer for Phoenix) and an order-placement
instruction builder, not a swap builder.

## Phase 4 — stable and weighted forks (3 registered)

`stabble_stable`, `stabble_weighted`, `mercurial_stable`. The math exists in
`beast-dex/src/math/`; only the account adapters are missing. These are the
cheapest wins after Phase 1 because the curve work is already done.

## Phase 5 — launchpads and bonding curves (4 registered)

`pump_fun`, `raydium_launchlab`, `meteora_dbc`, `alphaqa`. The bonding-curve
pricing is known (the legacy sniper implements it); what is missing is the
engine-side cycle path. Low volume per pool, high pool count.

## Non-DEX gaps that gate real trading

These are not venues but without them the venue work cannot produce a live
profitable trade.

1. **Deploy the guard program.** Source and host tests exist in `programs/guard`
   but no mainnet deployment. Without it, live mode needs
   `safety.allow_unguarded_live = true`, which removes the on-chain revert
   guarantee. This is the highest-value non-venue item.
2. **ALT lifecycle.** Two lookup tables exist (`33fhmzQ7…` for Jupiter Lend,
   `JAZCg2Pn…` for Marginfi) at 1,910,080 lamports each, plus a third is needed
   for any AMM-v4 mix. `beast flash-alt-close` recovers them; nothing runs it.
3. **Non-WSOL cycles.** Profit stays in the borrowed mint, so the lamport guard
   cannot be used and the margin comes from token-priced conversion. Needs a
   token-balance guard, or a settled sweep cycle.
4. **Rent hygiene.** The reclaimer is wired into `beast arb`; confirm empty
   token accounts do not accumulate across many passes.


## What actually blocks profit right now (measured, not assumed)

I ran the driver against mainnet continuously. These are the numbers, and the
one input that changes them.

| Search shape | Quotes priced | Profitable | Why |
|---|---|---|---|
| Two-pool CPMM cycles, SOL/USDC + SOL/USDT + SOL/JUP, sizes 0.1–14 SOL | 40,000+ | 0 | A two-pool cycle pays two pool fees and needs a gap between them larger than that; both pools are same-curve and same-bot-watched. |
| Same-pair Jupiter round trips | ~2,000 | 0 | Structurally zero: an aggregator picks the best route each way, so the return leg undoes the out leg's edge. |
| Triangular `WSOL -> a -> b -> WSOL` through Jupiter | 4–5 of 15 pairs per pass | 0 | The right shape — it crosses three different pairs so no single route can undo it — but only 4–5 pairs price per 45-second pass. |

The blocker on the triangles is **Jupiter's keyless endpoint, which allows 0.5
requests per second**. Each triangle needs three quotes, so a pass costs ~100
seconds of quoting and a sixth of the search space never gets priced at all.
An unpriceable triangle is now counted and logged per pass rather than
silently skipped, so a rate-limit ceiling is visible instead of looking like a
clean negative.

**A Jupiter API key raises that from 0.5 to roughly 30 requests per second —
a 60× increase in search rate.** That is the single highest-leverage input, and
nothing else about the scan is rate-limited.

## What I will not do

Ship a builder whose account list I have not verified. Every one of the venue
items above ends at a mainnet simulation, because a guessed account list does
not fail loudly — it either reverts, which costs a real fee, or worse it lands
incorrectly. Where a reference is missing (currently AMM v4) the item is marked
blocked rather than estimated.

## Ordering

`guard program` → `Phase 1.1 AMM v4` → `Phase 1 (rest)` → `Phase 4` → `Phase 3`
→ `Phase 2a` → `Phase 5` → `Phase 2b`.

The guard comes first because it is the difference between "found a profitable
cycle" and "executed one safely", and every later phase multiplies its value.
