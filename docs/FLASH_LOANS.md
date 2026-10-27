# Flash loans

SOL BEAST can fund a closed arbitrage cycle with an atomic Kamino flash loan.
The borrow and the repayment are instructions in the **same transaction** as the
Jupiter swaps, so a cycle that cannot return the principal plus the protocol fee
reverts instead of leaving a debt behind.

This document separates what was proven on mainnet from what is merely
implemented, and records why other Solana providers are not enabled yet.

## What is proven

| Item | Evidence |
|---|---|
| Kamino KLend program | `KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD`, executable on mainnet. |
| Borrow / repay instructions | Discriminators and account order verified against reference transaction `3ofZkLX2sXZgXujH5UPDMRa9h5JTQcmK7U9LKENFDTYhECW92hWBT6igpqCZuF2vaMsqbinpUnApzE4rsqcQxKor`. |
| Full round trip | `cargo run -p beast-flash --example roundtrip` simulates borrow → repay on mainnet with `err: none`. |
| Dynamic market authority | `["lma", lending_market]` PDA, checked against the live reserve; no hardcoded market authority. |
| Fee model | Reserve flash-fee bits are a 2<sup>60</sup> fixed-point fraction. Main market WSOL charges exactly **10 ppm** (10,000 lamports per 1 SOL), with ceiling rounding. |
| Main-market WSOL reserve | `d4A2prbA2whesmvHaL88BH6Ewn5N4bTSU2Ze8P6Bc4Q`, market `7u3HeHxYDLhnCoErrtycNokbQYbWGzLs6JSDqGAv5PfF`, about 202k SOL available at verification time. |
| Multi-token discovery | `cargo run -p beast-flash --example discover` decoded 630 reserves (487 flash-enabled) across 155 mints from `getProgramAccounts` with `dataSize = 8624`. |

The round-trip example is a **simulation**. No live flash transaction has been
signed and submitted by this repository. The distinction matters: simulation
proves the account list, discriminators and fee math are accepted by the
program; landing a bundle additionally depends on Jito, compute budget, wallet
balance and network conditions.

### Why the reference transaction is not the proof

The reference transaction's borrow succeeded but its swap failed with
`insufficient funds`, so it never reached repayment. The round-trip example
funds the destination associated token account with a small buffer, then
proves that Kamino accepts the repay instruction and charges the expected fee.
The buffer is required because Kamino debits **principal plus fee** from the
same token account, and a freshly created account has zero tokens.

## How the adapter works

The public surface is in `crates/beast-flash`:

```text
FlashProvider
  decode_reserve(address, data) -> FlashReserve
  borrow(reserve, wallet, user_token, amount, instruction_index) -> FlashInstruction
  repay(reserve, wallet, user_token, principal, borrow_index) -> FlashInstruction
  flash_fee(reserve, amount) -> u64
  repayment_amount(reserve, amount) -> u64
```

`KaminoFlashProvider` encodes the instruction data without importing any KLend
source or IDL. KLend is distributed under the Business Source License 1.1, so
copying its IDL or account structs into this MIT repository would be a licence
problem. The adapter uses field offsets that were read from live accounts and
validated by the mainnet simulation:

| Reserve field | Offset |
|---|---|
| lending market | 32 |
| liquidity mint | 128 |
| supply vault | 160 |
| fee vault | 192 |
| available liquidity | 224 |
| mint decimals | 272 |
| token program | 408 |
| status | 4856 |
| flash fee bits | 4904 |

The account list is identical for borrow and repay, which is how KLend pairs
the two halves of a flash operation:

```text
0  user transfer authority (wallet, signer)
1  lending market authority PDA
2  lending market
3  reserve
4  liquidity mint
5  reserve supply vault
6  user token account
7  reserve fee vault
8  referrer token state (program id: unused)
9  referrer account (program id: unused)
10 instructions sysvar
11 token program
```

`borrow_instruction_index` is the index of the borrow instruction **after** the
compute-budget and ATA-creation preamble. Repay takes that same index so KLend
can find its matching borrow.

## Fee arithmetic

A reserve stores `flash_fee_bits` as a fraction of 2<sup>60</sup>. A disabled
fee is `u64::MAX`; status `0` is active.

```text
fee = ceil(amount * flash_fee_bits / 2^60)
repayment = amount + fee
```

On the Kamino main market the WSOL value is `11529215046068`, which is exactly
10 ppm. Borrowing 1 SOL therefore costs 10,000 lamports. At that price, the old
assumption that flash loans cost 0.2–0.3% is wrong for this market by more than
two orders of magnitude.

Fees are charged in the borrowed token. For a non-WSOL cycle the fee is in that
token's raw units, and the engine converts both the profit and the fee into
lamports before comparing them with transaction fees, rent and the Jito tip.

## Enabling it

```toml
[flash]
enabled = true
provider = "kamino"
# Empty means "load the deepest active reserve for every Kamino mint".
# Set a mint (for example WSOL) to restrict discovery to one asset.
mint = ""
# Raw borrow cap. For WSOL these are lamports; for another mint these are the
# mint's raw units. WSOL's 9 decimals make the two identical.
max_borrow_lamports = 1000000000
# Empty means discovery. Pin a reserve to skip getProgramAccounts.
reserve = ""

[strategies.flash_arb]
mode = "radar"
```

`flash_arb` cannot be armed while `[flash] enabled = false`; validation rejects
that combination at startup. The provider id is also validated: only `kamino`
is accepted, so a typo cannot silently fall back to a different adapter.

At startup the engine either:

1. decodes the pinned `flash.reserve`, or
2. discovers reserves for the configured `flash.mint`, or
3. with both fields empty, discovers all KLend reserves and keeps the deepest
   active reserve per mint.

Every reserve is re-read immediately before a live cycle is built. Availability
and fees are governance-controlled state, so a startup snapshot is never used
to decide whether a borrow is safe.

## What the executor enforces

The flash transaction shape is:

```text
compute budget
[create the borrowed mint's ATA when absent]
flash borrow                    <- borrow_instruction_index
Jupiter leg 1 ... leg N         <- wrap/unwrap disabled
flash repay (principal)
[close the ATA]                 <- WSOL only
```

`prepare_flash_cycle` refuses the cycle unless:

- the route is closed (end mint == start mint);
- the route start mint has a loaded reserve;
- `amount_in` fits both `flash.max_borrow_lamports` and reserve availability;
- the reserve is active and its fee is not disabled;
- the final Jupiter leg is re-quoted so its **on-chain minimum out** covers
  principal + fee + minimum profit + base fees + rent + Jito tip;
- the wallet can pay the SOL-denominated costs and keep the balance floor.

For WSOL the destination ATA is closed after repayment, which moves the profit
back to the wallet as lamports. That makes the separate lamport guard usable and
the bundle keeps the usual `cycle, guard, tip` shape.

For non-WSOL cycles the profit stays in the borrowed token. A lamport guard
cannot observe it, so the engine deliberately omits that guard and reports
`lamport_guard = false` in the plan and in `/v1/status`. The protection is the
enforced final minimum out plus the repayment instruction itself: if the token
account cannot cover principal + fee, the whole transaction reverts.

## Tokens and venues

Any mint with an active Kamino reserve can be borrowed. The route still has to
be discoverable by Radar, which means at least one configured/decoded pool and
a closed path back to the same mint. Jupiter supplies the swap instructions;
Kamino supplies only the loan.

The following are **not** enabled and are not presented as working:

| Provider | Status |
|---|---|
| Solend / Save | Its own docs describe flash loans as limited by Solana reentrancy and point to unfinished work. Not enabled. |
| Marginfi v2 | The documented mainnet program id did not resolve at verification time. Not enabled. |
| Drift | Has flash-loan-like mechanics, but no adapter has been proven here. Not enabled. |
| Port / Larix | No verified flash adapter in this repository. Not enabled. |
| Other DEXes | Swaps are reached through Jupiter; that is not a flash-loan provider. |

A provider is added only after its account layout and a full borrow → repay
round trip are accepted by a mainnet simulation. Cross-provider fallback is not
implemented: a reserve or provider that fails verification fails startup rather
than being silently skipped.

## What is still unproven

1. **No live bundle has been sent.** Simulation proves program acceptance, not
   Jito landing.
2. **Non-WSOL live cycles have no lamport guard.** Their atomicity comes from
   the repayment instruction, not from the guard program.
3. **Availability moves.** A reserve can be paused, drained or have its fee
   changed between planning and landing. The executor re-reads it before
   building, and the repay instruction is the final on-chain check.
4. **Compute budget is an estimate.** The flash cycle reserves 1.4M compute
   units. A pathologically large Jupiter route may need more; the bundle
   simulation surfaces that before submission.
5. **Jito tip and priority fee are estimates.** The guard and min-out use the
   configured worst case; network conditions can still make a simulated bundle
   fail to land.
