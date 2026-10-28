# Flash loans

SOL BEAST can fund a closed arbitrage cycle with an atomic Kamino or Solend
flash loan. The borrow and the repayment are instructions in the **same
transaction** as the swap legs, so a cycle that cannot return the principal plus
the protocol fee reverts instead of leaving a debt behind.

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
| Solend program | `So1endDq2YkqhipRh3WViPa8hdiSpxWy6z3Z6tMCpAo`, executable on mainnet. |
| Solend instruction tags | Borrow `19` and repay `20` accepted by the program; logs show `Instruction: Flash Borrow Reserve Liquidity` and `Instruction: Flash Repay Reserve Liquidity`. |
| Solend full round trip | `cargo run -p beast-flash --example solend_roundtrip` simulates borrow → repay on mainnet with `err: none`. |
| Solend fee model | Reserve fee is a 1e18 WAD fraction, rounded to nearest. The main WSOL reserve charges exactly **500 ppm**: borrowing 100,000,000 lamports cost 50,000, split 40,000 reserve / 10,000 host. |
| Solend WSOL reserve | `8PbodeaosQP19SjYFx855UMqWxH2HynZLdBXmsrbac36`, market `4UpD2fh7xH3VP9QQaXtsS1YY3bxzWhtfpks7FatyKvdY`, about 77,780 SOL available at verification time. |
| Solend engine integration | `beast flash-cycle --probe` with `provider = "solend"` built and simulated a full Solend borrow → two Raydium CPMM swaps → Solend repay cycle with `ok: true`. |
| Marginfi v2 program | `MFv2hWf31Z9kbCa1snEPYctwafyhdvnV7FZnsebVacA`, executable on mainnet. |
| Marginfi flash shape | No dedicated instruction: an account flag (`LendingAccountStartFlashloan` / `LendingAccountEndFlashloan`) wraps a normal borrow/repay pair. Inside the flag `check_account_health` returns `Ok(())`, so a fresh zero-collateral account can borrow and repay. |
| Marginfi discriminators | All six discriminators (`InitializePda`, `Close`, `StartFlashloan`, `Borrow`, `Repay`, `EndFlashloan`) verified against the Anchor instruction hashes and accepted by mainnet simulation. |
| Marginfi fee model | `NoFee`. The flash fee is zero; the borrow `protocol_origination_fee` is an I80F48 charged as extra debt. All current WSOL banks report 0, so the adapter rejects nonzero-origination-fee banks. |
| Marginfi WSOL bank | `CCKtUs6Cgwo4aaQUmBPmyoApH2gUDErxNZCAntD6LYGh` (group `4qp6Fx6tnZkY5Wropq9wUYgtFxXKwE6viZxFHg3rdAG8`, vault `2eicbpitfJXDwqCuFAmPgDP7t2oUotnAzbGzRKLMgSLe`, authority `DD3AeAssFvjqTvRTrRAtpfjkBF8FpVKnFuwnMLN9haXD`, Pyth oracle `7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE`), about 6,243 SOL available at verification time. |
| Marginfi round trip | `cargo run -p beast-flash --example marginfi_roundtrip` simulates init PDA → start flashloan → borrow → `repay_all` → end → close account → close ATA on mainnet with `err: none`. |
| Marginfi engine integration | `beast flash-cycle --probe` with `provider = "marginfi"` simulated a full Marginfi borrow → two Raydium CPMM swaps → repay cycle with `ok: true`, using an address lookup table. |
| Jupiter Lend program | `jupgfSgfuAXv4B6R2Uxu85Z1qdzgju79s6MfZekN6XS`, executable on mainnet and owner of the admin account. |
| Jupiter Lend flash shape | A dedicated program, separate from the liquidity program: exactly two instructions, `flashloanBorrow` and `flashloanPayback`, and **no** borrow index — the protocol walks the `instruction_sysvar` to find its own borrow. |
| Jupiter Lend fee | Protocol-wide `flashloan_fee` is **0 basis points right now**, so repayment is exactly the principal. Zero is a real value read from the live admin, not the "disabled" marker. |
| Jupiter Lend full round trip | `cargo run -p beast-flash --example jupiter_roundtrip` simulates borrow → repay on mainnet with `err: none` and a net wallet cost of **6,400 lamports** (ATA rent only). |
| Jupiter Lend engine integration | `beast flash-cycle --probe` with `provider = "jupiter"` simulated borrow → two Raydium CPMM swaps → repay with `ok: true`, `err: null`, fee 0, at 1050 bytes. |
| Jupiter Lend liquidity | WSOL vault holds **~395,828 SOL**; `available` is bounded by `supply * max_utilization - borrow`, then by the borrow position's `max_debt_ceiling - amount`, then by the vault's real token balance. |
| Jupiter ALT | `beast flash-alt --real --confirm` with `provider = "jupiter"` created table `33fhmzQ7H7ZDaKREnV1x79Xp43drPT8GiETKuAWzNFkr` on mainnet, rent 1,910,080 lamports, refundable with `beast flash-alt-close`. |
| Marginfi transaction size | The full lifecycle plus two CPMM swaps is 1326 bytes of static keys, over the 1232-byte limit. A lookup table holding six long-lived Marginfi accounts brings it to **1177 bytes**. |
| ALT setup | `beast flash-alt --real --confirm` created table `JAZCg2PnEKTkBDVuC25vtEA28zhAFHKm3cHrX3VdyfqU` on mainnet (slot 462194404, signature `gzhC2ZkMRXBsmKfQr9jd8nUbBhApCCG3vtGCV6SGDyV48Dfspwb8LRp6BYknwsjL1DeSJbGacDnR6vi9XjbCqMg`), rent 1,910,080 lamports paid and refundable via `beast flash-alt-close`. |

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

| Kamino reserve field | Offset |
|---|---:|
| lending market | 32 |
| liquidity mint | 128 |
| supply vault | 160 |
| fee vault | 192 |
| available liquidity | 224 |
| mint decimals | 272 |
| token program | 408 |
| status | 4856 |
| flash fee bits | 4904 |

The Kamino account list is identical for borrow and repay, which is how KLend
pairs the two halves of a flash operation:

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

### Solend / Save

`SolendFlashProvider` uses tags `19` (borrow) and `20` (repay) and a different,
nine-account repay list:

| Solend reserve field | Offset |
|---|---:|
| version | 0 |
| lending market | 10 |
| liquidity mint | 42 |
| mint decimals | 74 |
| supply vault | 75 |
| available liquidity | 171 |
| flash loan fee (WAD) | 314 |
| host fee percentage | 322 |
| fee receiver **token account** | 339 |

The account lists are:

```text
borrow                                   repay
0 supply vault (writable)                 0 source token account (writable)
1 destination token account (writable)    1 supply vault (writable)
2 reserve (writable)                      2 reserve fee receiver (writable)
3 lending market                          3 host fee receiver (writable)
4 market authority PDA                    4 reserve (writable)
5 instructions sysvar                     5 lending market
6 token program                           6 wallet (signer)
                                          7 instructions sysvar
                                          8 token program
```

The authority is `find_program_address([market], program)`; for the main market
that is `DdZR6zRFiUt4S5mg7AV1uKB2z1f1WzcNYCaTEEWPAuby`, bump 255, which owns the
live supply vault. The fee receiver at offset 339 is the token account
`5wo1tFpi4HaVKnemqaXeQnBEpezrJXcXvuztYaPhvgC7` (owned by `9Ruq...`), not the
authority pubkey. Passing it for both the host and reserve fee destinations was
accepted by the program and produced the exact 40,000 / 10,000 split.

`borrow_instruction_index` is the index of the borrow instruction **after** the
compute-budget and ATA-creation preamble. Repay takes that same index so the
lender can find its matching borrow. Both adapters expose the repay argument as
the **principal**; the fee is charged separately from the source token account.

## Fee arithmetic

Each reserve declares its raw fee and a fee model. A disabled fee is
`u64::MAX` on both protocols.

Kamino stores a 2<sup>60</sup> fraction and rounds **up**:

```text
fee = ceil(amount * flash_fee_bits / 2^60)
```

On the Kamino main market the WSOL value is `11529215046068`, which is exactly
10 ppm. Borrowing 1 SOL therefore costs 10,000 lamports.

Solend stores a 1e18 WAD fraction and rounds to **nearest**, with a minimum of
two token units when a host fee is configured:

```text
fee_decimal = max(amount * flash_fee_bits, minimum_units * 1e18)
fee         = round(fee_decimal / 1e18)
```

The verified Solend WSOL reserve has `flash_fee_bits = 500000000000000`
(500 ppm) and a 20% host cut. Borrowing 100,000,000 lamports cost 50,000:
40,000 to the reserve and 10,000 to the host. On this reserve a fee-bearing
borrow must be at least 3 lamports, because the two-unit minimum must stay
strictly below the principal.

`repayment_amount` is principal + fee. The repay instruction itself still takes
the principal; the source account must hold principal + fee.

Jupiter Lend is the fourth model, and structurally the cheapest: the fee is
**protocol-wide**, not per-reserve, so one admin account decides it for every
mint:

```text
fee = ceil(amount * flashloan_fee_bps / 10_000)
```

The verified value on mainnet is `0`, so a Jupiter Lend cycle repays exactly the
principal and the fee term in the profit model is zero. The adapter reads the
live admin account before every cycle, so if the protocol is reconfigured to a
nonzero fee the profit model follows automatically instead of quietly
under-charging. A **paused** protocol is reported as paused, never as a free
loan.

Fees are charged in the borrowed token. For a non-WSOL cycle the fee is in that
token's raw units, and the engine converts both the profit and the fee into
lamports before comparing them with transaction fees, rent and the Jito tip.

## Enabling it

```toml
[flash]
enabled = true
provider = "kamino"    # or "solend", "marginfi" or "jupiter"
# Address lookup table for providers whose static instruction set exceeds
# Solana's 1232-byte transaction limit. Create it with `beast flash-alt`,
# then paste the table address it prints here. Empty means "no lookups".
# Marginfi + two CPMM swaps needs one: 1326 bytes of static keys without a
# table (rejected), 1177 bytes with. Jupiter Lend + two CPMM swaps also needs
# one: 1264 bytes without, 1050 with.
lookup_table = ""
# Empty means "load the deepest active reserve for every mint of this provider".
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
that combination at startup. The provider id is also validated: only `kamino`,
`solend`, `marginfi` and `jupiter` are accepted, so a typo cannot silently fall
back to a different adapter.

When a cycle's instruction set is too large for one transaction, the composed
cycle is assembled as a **v0** transaction with the configured lookup table.
The table is created once by `beast flash-alt` and is a plain cost: rent that
the wallet pays and recovers. Deactivate and close it later with
`beast flash-alt-close --table <address>` (first deactivates, then closes after
the slot cool-down, returning the rent).

At startup the engine either:

1. decodes the pinned `flash.reserve`, or
2. discovers reserves for the configured `flash.mint`, or
3. with both fields empty, discovers all reserves and keeps the deepest active
   reserve per mint.

The WSOL default means the common path is a single mint-filtered
`getProgramAccounts`. Helius accepts the mint-filtered call for Solend but
deprioritises the unfiltered one because Solend has hundreds of reserves; pin
`flash.reserve` (or keep a mint set) on a strict free tier.

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

Any mint with an active Kamino, Solend, Marginfi or Jupiter Lend reserve can be
borrowed. The route still has to be discoverable by Radar, which means at least one
configured/decoded pool and a closed path back to the same mint. The provider
supplies only the loan; swaps come from Jupiter or the local CPMM builder.

Marginfi needs one extra piece of plumbing that Kamino and Solend do not: a
temporary account PDA is created, used for the borrow/repay and closed inside
the same transaction. Its rent (12,395,200 lamports) floats out of the wallet
and comes back when the account closes, so it is included in the balance check
but not in the profit model. The account index is picked by probing PDAs until
an unused one is found, so a leftover account at index 0 does not break a cycle.

The following are **not** enabled and are not presented as working:

| Provider | Status |
|---|---|
| Drift | Has flash-loan-like mechanics, but no adapter has been proven here. Not enabled. |
| Port / Larix | No verified flash adapter in this repository. Not enabled. |
| Jupiter Lend (`@jup-ag/lend`) | Fee-less flash loans exist, but the SDK is JavaScript and no borrow → repay round trip has been proven from this repository. Not enabled. |
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
6. **Solend has no status byte.** Its adapter treats `available > 0` and a
   non-disabled fee as borrowable. A paused reserve that still reports
   liquidity would be rejected by the program atomically, not by the planner.
