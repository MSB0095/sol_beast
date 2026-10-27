# Flash loans: research findings and current status

This document records what was verified, why no flash-loan adapter is shipped,
and what was built instead. It exists so the reasoning is auditable rather than
implied.

## What was verified (October 2026)

| Protocol | Program ID | Status |
|---|---|---|
| Kamino Lending (KLend) | `KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD` | **verified executable on mainnet** by `getAccountInfo`; exposes `flash_borrow_reserve_liquidity` / `flash_repay_reserve_liquidity` (no V2 variant). |
| Kaito/Solend (Save) | — | Solend's own flash-loan docs state the feature is *"limited due to the reentrancy of Solana transactions"* and point to an unfinished PR. Not production-viable. |
| Marginfi v2 (mrgnLend) | `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacASt` | **documented on their program-addresses page but does not resolve on mainnet** (`getAccountInfo` returns null). Staging resolves; mainnet does not. Treated as unverified. |

### Licence constraint

The KLend repository is distributed under the **Business Source License 1.1**,
not an OSI-approved licence. Vendoring its IDL or porting its instruction
layouts into this workspace would be a licence violation, so no KLend code or
account-layout definitions are copied here.

## Why no flash-loan adapter ships today

1. **No legally-clean, verified provider.** KLend is BUSL; Solend is
   self-described as incomplete; Marginfi's documented mainnet ID does not
   resolve. Shipping an adapter against any of them would mean either a licence
   breach or an unverifiable account layout.
2. **The economics do not favour it at small capital.** A Solana flash loan
   charges roughly 0.2–0.3% of the *borrowed* amount. A cycle that borrows
   1 SOL to rescue a 0.01-SOL dislocation pays ~0.002–0.003 SOL in loan fees,
   which is larger than the entire gross profit. Flash lending pays off at
   large notional sizes, not at the capital levels this engine was first tested
   at.
3. **The guard already provides the safety property flash loans are used for.**
   The on-chain guard compares the wallet's real post-swap lamports against
   `expected_start + min_profit + tip + remaining_fee`, so a losing cycle
   reverts regardless of where the input capital came from. A flash loan adds
   atomicity, not safety.

## What was built instead: composed multi-leg arbitrage

`crates/beast-exec/src/compose.rs` removes the real constraint flash loans were
meant to address — **capital efficiency per transaction** — without needing a
borrow at all.

Previously an N-leg cycle cost N Jupiter *transactions* plus a guard and a tip,
so Jito's five-transaction bundle cap limited a cycle to three legs. Composition
puts all N legs of raw instructions into **one** transaction:

| | Per-leg bundles | Composed cycle |
|---|---|---|
| Leg ceiling | 3 (Jito 5 tx − guard − tip) | bounded by the 1232-byte transaction size |
| Network fees per cycle | one per leg | **one** |
| Bundle shape | `swap1..swapN, guard, tip` | `cycle, guard, tip` |

The transaction must be **v0** with address lookup tables: a single Jupiter
WSOL→USDC hop already references ~25 accounts, and a legacy message costs 32
bytes per unique account key, so two composed legs overflow 1232 bytes while v0
costs one index byte per account. `assemble_v0` implements the documented
static → writable-table → readonly-table index concatenation.

Measured on mainnet (`cargo run -p beast-exec --example compose_live`):
- a live 2-leg quote resolves to real routes (Meteora DLMM, Kipseli) with
  `minOut` values;
- instruction sets fetch cleanly;
- **Jupiter's own `simulationError` gate rejects a cycle whose user wallet
  cannot simulate it** — the client surfaces that as an error instead of
  building a doomed transaction, which is exactly the intended behaviour.

## If a flash-loan provider becomes viable

The cost model already has the shape it needs: add `flash_fee_lamports` to
`CostBreakdown`, include it in `choose_tip`'s affordability check, and keep the
guard threshold unchanged (it compares real balances, so it is indifferent to
where the capital came from). A provider adapter would implement borrow/repay as
two instructions wrapped around the composed cycle, inside the same transaction
as the guard.

That adapter ships only when a provider is both verified on-chain and
distributable under a licence this project can ship under.
