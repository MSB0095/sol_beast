# BEAST on-chain guard

Native Solana program that enforces the final wallet balance before the Jito
tip transaction:

```
final_lamports >= expected_start + min_profit + tip + fees
```

The bundle must be ordered `swap1, swap2, guard, tip` (or more swaps followed
by guard and tip). The guard instruction has one account: the wallet
(writable, signer). Instruction data is 33 bytes:

```
u8 version (=1)
u64 expected_start   LE
u64 min_profit       LE
u64 tip              LE
u64 fees             LE
```

## Build

```sh
cargo build-sbf --manifest-path programs/guard/Cargo.toml
```

## Deploy

Program IDs are deployment-specific. This repository intentionally does not
ship a funded keypair or a fixed program ID. Generate one, deploy, then put the
resulting program ID in `config.toml` under `[guard] program_id = "..."` and set
`mode = "program"`.

```sh
solana-keygen new -o programs/guard/guard-keypair.json
cargo build-sbf --manifest-path programs/guard/Cargo.toml
solana program deploy \
  --program-id programs/guard/guard-keypair.json \
  programs/guard/target/deploy/beast_guard.so
```

The keypair and build output are ignored by git. A program ID is only trusted
after the deployed bytes have been verified and the bundle has passed a fork
test.

The guard is not a substitute for simulation. The executor still simulates the
bundle and the Jito block engine drops bundles whose guard instruction fails.
