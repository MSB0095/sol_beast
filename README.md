<div align="center">

# 🦁 SOL BEAST 2.0

### Green-or-revert Solana keeper / MEV engine

*Radar, simulate, guard, bundle — or don't send.*

[![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/react-%2320232a.svg?style=for-the-badge&logo=react&logoColor=%2361DAFB)](https://reactjs.org/)
[![Solana](https://img.shields.io/badge/Solana-000?style=for-the-badge&logo=solana&logoColor=9945FF)](https://solana.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg?style=for-the-badge)](https://opensource.org/licenses/MIT)

[Quick start](#-quick-start) • [Safety model](#-safety-model) • [Venues](#-venues) • [Documentation](docs/V2.md) • [Legacy sniper](docs/LEGACY.md)

</div>

---

## What this is

SOL BEAST 2.0 is a Solana liquidity keeper. It watches configured pools, prices
closed arbitrage cycles with local AMM models, asks Jupiter for execution
quotes, simulates the Jito bundle, and only submits when every modelled cost is
covered. If the on-chain guard does not see the required profit, the bundle
reverts instead of paying the tip.

The original pump.fun sniper still ships as the legacy root `sol_beast` binary.
It is documented separately in [docs/LEGACY.md](docs/LEGACY.md).

**No sandwiches. No frontrunning. No pool manipulation. No claims of live
profitability without live settlement data.**

## What ships

- **Radar** — local pool feed, reserve refresh, AMM/CLOB math, route search,
  cost model and persistence.
- **60-program venue registry** — every program ID checked mainnet-executable.
  Coverage is explicit: `modeled`, `decoded`, `stake_pool`, `quote_only`,
  `catalogued`, `disabled`.
- **Six strategies** — `cross_dex_arb`, `triangular_arb`, `lst_arb`,
  `stable_arb`, `flash_arb`, `rent_reclaim`.
- **Kamino flash loans** — borrow, swap and repay in one transaction, so an
  unprofitable cycle reverts instead of leaving a debt. Verified by a mainnet
  borrow → repay simulation; see [docs/FLASH_LOANS.md](docs/FLASH_LOANS.md).
- **Jito bundle execution** — 2–3 leg closed cycles, one swap transaction per
  leg, an on-chain guard transaction, then the tip. Jito caps bundles at 5
  transactions; longer and flash cycles are composed into one v0 transaction.
- **Native guard program** — no admin, no upgrade authority, no token accounts.
  Deploy-time program ID, documented build/deploy steps.
- **Settlement from chain** — real pre/post balance deltas, model-error
  tracking, automatic halt on a landed negative net.
- **Hardened API** — loopback default, distinct admin/read tokens, CORS
  allowlist, rate limits, redacted config, optional static frontend serving.
- **Rent reclaimer** — closes only initialized, zero-balance token accounts
  owned by the wallet; dry run unless explicitly real.
- **React dashboard** — login, dashboard, radar, execution, rent, venues,
  pools, strategies and settings, all wired to the real API.

Not shipped and not presented as adapters: liquidation auctions, backrunning,
oracle-lag trading, sandwiching and pool manipulation.

## Safety model

Live execution requires **all** of:

1. `runtime.real = true` in config.
2. `--real` on the command line.
3. `runtime.mode = "live"` (or a live strategy override).
4. Jito enabled and reachable.
5. `guard.mode = "program"` with a deployed `guard.program_id`.

Otherwise the engine stays in Radar/Canary and only simulates. The API signing
endpoints are disabled unless those gates are satisfied. `safety.allow_unguarded_live`
can deliberately remove gate 5; that path has no on-chain revert guarantee and is
off by default.

A flash cycle always keeps its borrow and repayment in the same transaction, so
the program itself rejects a cycle that cannot repay principal plus fee. WSOL
flash cycles additionally use the guard. Non-WSOL flash cycles keep profit in
the borrowed mint, which a lamport guard cannot observe; they are reported with
`lamport_guard = false` and rely on the enforced final minimum out plus the
repayment instruction.

The guard enforces, at instruction execution time:

```
final_lamports >= expected_start + min_profit + tip + remaining_fee
```

Swap fees and ATA rent already paid are reflected in `final_lamports`, so they
are not subtracted twice. The tip transaction is the only future cost reserved.

Jito bundles are atomic in normal operation. Jito documents that uncled blocks
can be re-proposed and individual transactions can land outside the bundle; that
is a protocol-level residual risk for any multi-transaction bundle. The engine
does not hide it. See [docs/V2.md](docs/V2.md#bundle-atomicity-caveat).

## Quick start

```bash
# Build and validate.
cargo build --release -p beast-cli
cargo run --release -p beast-cli -- check

# Write config.toml + secrets.toml (0600, two distinct API tokens).
cargo run --release -p beast-cli -- init

# Discover one WSOL/USDC pair for a locally modelled venue.
cargo run --release -p beast-cli -- discover --venue raydium_cpmm \
  --mint So11111111111111111111111111111111111111112 \
  --mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v \
  --limit 50 --save

# Radar: price routes and persist opportunities; never send.
cargo run --release -p beast-cli -- run

# Simulate a local two-leg WSOL flash cycle through two Raydium CPMM pools.
# Add --probe only to prove mechanics with a prefunded, deliberately
# unprofitable cycle; both commands simulate and never send.
cargo run --release -p beast-cli -- flash-cycle \
  --pool-in <cpmm-pool-a> --pool-out <cpmm-pool-b> --amount 1000000
```

Frontend:

```bash
cd frontend
npm ci
npm run dev          # http://localhost:3000, /v1 -> 127.0.0.1:8081
```

Production single-process serving:

```bash
cd frontend && npm ci && npm run build && cd ..
# config.toml: [api] static_dir = "frontend/dist"
cargo run --release -p beast-cli -- run
# open http://127.0.0.1:8081
```

Log in with the admin token from `secrets.toml`.

## Venues

The registry includes 60 Solana programs: 8 locally modelled, 3 locally
decoded, 2 stake-pool rate sources, 40 Jupiter quote-only venues, 6 catalogued
programs without a shipped adapter, and 1 disabled/deprecated program. All 60
program IDs were verified executable with mainnet `getMultipleAccounts` at the
time of writing.

`quote_only` venues are reachable through Jupiter execution; they are not
locally priced by Radar. `catalogued` and `disabled` venues never route.
Registration is not a claim of local execution.

## Execution pipeline

1. Reject open routes and routes longer than 3 legs; capital-funded routes must
   start and end at WSOL, while a flash route must start and end at its borrowed
   mint.
2. Check wallet balance against input + `risk.balance_floor_lamports`.
3. Request direct Jupiter quotes; require `minOut > 0` and cycle output > input.
4. Price all costs: base fees, intermediate ATA rent, Jito tip, minimum profit
   and, for flash cycles, the reserve fee.
5. Refuse if the tip floor, fees, rent and profit cannot be covered.
6. Build signed swap transactions, a guard transaction and a tip transaction —
   or one composed v0 transaction plus guard and tip for triangular/flash cycles.
7. Simulate the bundle via Jito.
8. Submit only when every live gate is satisfied.
9. Poll status, reconcile from on-chain balance deltas, persist settlements and
   halt on a landed negative net.

## Guard deployment

No fixed program ID is shipped. Generate one at deploy time:

```bash
solana-keygen new -o programs/guard/guard-keypair.json
solana address -k programs/guard/guard-keypair.json
cd programs/guard && cargo build-sbf
solana program deploy --url <rpc> --keypair <payer> \
  --program-id guard-keypair.json target/deploy/beast_guard.so
```

Then:

```toml
[guard]
enabled = true
mode = "program"
program_id = "<deployed program id>"
```

The environment used to prepare this repository did not include
`solana-keygen` or `cargo-build-sbf`, so a real deployment and mainnet Jito
bundle are **not** claimed as validated. Run a fork test and a small live bundle
before scaling.

## API

Loopback default: `127.0.0.1:8081`. Remote binding requires
`api.allow_remote = true` plus a non-empty `api.allowed_origins`.

Authenticate with `Authorization: Bearer <token>` or `x-api-token: <token>`.
Admin and read tokens are distinct. See [docs/V2.md](docs/V2.md#http-api) for the
full route list.

The API serves a built frontend when `api.static_dir` points at a directory
containing `index.html`. Unknown `/v1/*` paths remain 404.

## Testing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cargo fmt --manifest-path programs/guard/Cargo.toml -- --check
cargo clippy --manifest-path programs/guard/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path programs/guard/Cargo.toml

cd frontend && npm ci && npm run lint && npm run build
```

CI runs the same checks.

## Documentation

- [docs/V2.md](docs/V2.md) — architecture, config, API, venues, execution,
  guard, rent, testing and known gates.
- [docs/FLASH_LOANS.md](docs/FLASH_LOANS.md) — verified Kamino adapter, fee math
  and the providers that are deliberately not enabled.
- [docs/RPC.md](docs/RPC.md) — free-tier provider assessment, role routing and
  zero-cost configuration.
- [docs/LEGACY.md](docs/LEGACY.md) — original pump.fun sniper.
- [SOL_BEAST_ROADMAP.md](SOL_BEAST_ROADMAP.md) — design roadmap. Future phases
  are plans, not shipped features.

## License

MIT. See [LICENSE](LICENSE).

---

*Trade smart. Trade fast. Or revert. — SOL BEAST 2.0*
