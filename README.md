# sol_beast

Tiny Rust async service to monitor pump.fun events on Solana, auto-buy under heuristics and manage holdings (TP/SL/timeout).

Quick start

1. Copy the example config and edit values (RPC/WS URLs and program IDs):

```bash
cp config.example.toml config.toml
# edit config.toml and set wallet_keypair_path before using --real
```

2. Run in dry (safe) mode — this will NOT use any wallet or send transactions:

```bash
RUST_LOG=info cargo run
```

3. Run in real mode (ONLY after you set `wallet_keypair_path` in `config.toml` to a secure keypair file):

```bash
RUST_LOG=info cargo run --release -- --real
```

Notes & safety

- The `--real` path uses the keypair file at `wallet_keypair_path`. Do not commit private keys to the repository.
- `rpc::buy_token` and `rpc::sell_token` contain TODOs and placeholder `Instruction` data — review and implement proper transaction construction before enabling `--real` in any automated environment.

Files of interest

- `src/main.rs` — runtime, message processing and holdings monitor
- `src/ws.rs` — websocket subscriptions and reconnect loop
- `src/rpc.rs` — Solana RPC helpers, price extraction, buy/sell functions (TODOs)
- `src/models.rs` — bonding curve state and models
- `config.example.toml` — example configuration (copy to `config.toml`)
