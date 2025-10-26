<!-- Purpose: Guidance for AI coding agents working on the `sol_beast` repo -->
# copilot-instructions for sol_beast

This repository is a small Rust async service that monitors Solana on-chain events (pump.fun program), buys newly created tokens under heuristics, and manages holdings (sell on TP/SL/timeout). The guidance below highlights the essential patterns and places to be careful when making code changes.

1) Big picture
- Entry: `src/main.rs` — orchestrates runtime, spawns the holdings monitor and one WS task per `solana_ws_urls` from config.
- Websockets: `src/ws.rs` — connects to Solana websocket endpoints, subscribes to logs and account updates, sends raw JSON messages into the mpsc channel consumed by `main`.
- RPC & on-chain logic: `src/rpc.rs` — all HTTP RPC calls, metadata decoding, price extraction from bonding-curve accounts, and functions `buy_token` / `sell_token` (these contain TODOs for real tx construction).
- Models & caches: `src/models.rs` — `BondingCurveState`, `Holding`, and `PriceCache` type alias (LruCache). Concurrency uses `Arc<Mutex<...>>` and `tokio` primitives.
- Settings: `src/settings.rs` — loads `config.toml` via the `config` crate. Required keys (see below).

2) Critical developer workflows
- Build: `cargo build` (or `cargo build --release`). The repo is standard Cargo-based (see `Cargo.toml`).
- Run (dry / safe): place a `config.toml` next to the binary and run: `RUST_LOG=info cargo run` — this will not send transactions because the `--real` flag is required to enable wallet usage.
- Run (real mode): ensure `config.toml` `wallet_keypair_path` points to a valid Solana keypair file, then: `RUST_LOG=info cargo run --release -- --real`.
- Logging: `env_logger::init()` is used; set `RUST_LOG=info` or `RUST_LOG=debug` for more detail.

3) Project-specific conventions & patterns
- Async-first: the app uses Tokio (multi-threaded runtime), `mpsc` channels for incoming WS messages, and `Arc<Mutex<...>>` for shared state.
- Caching: `lru::LruCache` stores seen signatures and price cache entries. Settings control `cache_capacity` and `price_cache_ttl_secs`.
- Multi-RPC fallback: `rpc::fetch_with_fallback` uses `select_ok` over `solana_rpc_urls` — change/add URLs in `config.toml` to increase resilience.
- Detection heuristic: new tokens are detected by scanning transaction logs for the literal "Program log: Instruction: InitializeMint2" and by parsing inner_instructions for the configured `pump_fun_program` — see `process_message` and `rpc::fetch_transaction_details`.

4) Integration points & external dependencies
- Solana RPC & WebSockets: configured in `config.toml` as `solana_rpc_urls` and `solana_ws_urls`.
- Programs: `pump_fun_program` and `metadata_program` are required program IDs in the settings.
- Metadata decoding: uses `mpl-token-metadata::accounts::Metadata` and base64 decoding of account data in `rpc::fetch_token_metadata`.
- crates: see `Cargo.toml` (tokio, tokio-tungstenite, reqwest, solana-client/sdk/program, mpl-token-metadata, borsh, serde, lru, etc.).

5) Important TODOs / safety notes (must be respected by agents)
- The real transaction paths in `rpc::buy_token` and `rpc::sell_token` have placeholder `Instruction` data and TODO comments — do NOT enable `--real` on CI or assume buys/sells are safe until these are implemented and thoroughly reviewed. The code currently constructs empty `Instruction` items when `is_real` is true.
- `handle_new_token` currently uses a hard-coded buy attempt of `0.1` SOL; adjust logic there if you change buy sizing.
- WS account subscription mapping in `ws.rs` contains a comment noting sub IDs don't map back to mints cleanly — be cautious when changing subscriptions.

6) Where to look for examples in this repo
- Event detection and processing: `src/main.rs::process_message` and `src/main.rs::handle_new_token`.
- RPC fallbacks & metadata: `src/rpc.rs::fetch_with_fallback`, `fetch_transaction_details`, `fetch_token_metadata`, and `fetch_current_price`.
- Price computation & bonding curve parsing: `src/models.rs::BondingCurveState` and `rpc::fetch_current_price`.
- Websocket reconnection pattern: `src/main.rs` spawns `ws::run_ws` in a reconnect loop with randomized backoff.

7) Quick reference: required `config.toml` keys
- solana_ws_urls: ["wss://...", ...]
- solana_rpc_urls: ["https://...", ...]
- pump_fun_program: "<pubkey>"
- metadata_program: "<pubkey>"
- wallet_keypair_path: "./keypair.json"
- tp_percent: float (take-profit percent)
- sl_percent: float (stop-loss percent)
- timeout_secs: integer (holding timeout)
- cache_capacity: integer
- price_cache_ttl_secs: integer

8) Example commands
- Build: cargo build
- Run (dry): RUST_LOG=info cargo run
- Run (real): RUST_LOG=info cargo run --release -- --real

9) When you (the AI agent) should ask before changing code
- Any change that enables or changes transaction construction or the `--real` path (wallet/keypair usage). Ask for human approval and a plan for secure key handling.
- Changes to detection heuristics that could increase false positives (e.g., changing the log string or lowering fee checks).

If anything here is unclear or you'd like me to include examples of `config.toml` or annotate the exact lines referenced, tell me what to add and I'll iterate.

---

## Sample `config.toml` (example)
Create `config.example.toml` in the repo root and copy to `config.toml` when ready. Replace placeholders before running in `--real` mode.

```toml
solana_ws_urls = ["wss://api.mainnet-beta.solana.com/"]
solana_rpc_urls = ["https://api.mainnet-beta.solana.com/"]
pump_fun_program = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
metadata_program = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
wallet_keypair_path = "./keypair.json"
tp_percent = 30.0
sl_percent = -20.0
timeout_secs = 3600
cache_capacity = 1024
price_cache_ttl_secs = 30
```

## Line-accurate pointers (where to edit / inspect)
Use these exact line ranges when making changes or reviewing behavior.

- `src/settings.rs` — config structure and loader: lines 3–15 and 17–23.
- `src/main.rs`
	- program entry, runtime & flags: lines 18–36 (arg parsing, keypair loading).
	- websocket spawn / reconnect loop: lines 56–81 and run invocation lines 64–75.
	- message detection (log string): lines 98–116 (look for the InitializeMint2 log at line 113).
	- buy decision & hard-coded 0.1 SOL: lines 124–141 (buy call at line 137).
	- holdings monitor (TP/SL/timeout logic): lines 148–201 (sell call at line 188).

- `src/rpc.rs`
	- transaction parsing (extract creator + mint): lines 31–73 (inner instruction parsing around lines 50–63).
	- token metadata fetch: lines 75–99.
	- RPC fallback helper: lines 101–127.
	- price extraction / bonding curve parsing: lines 129–175 (deserialize at lines 156–158; `state.complete` check at 159–161).
	- buy path and TODO for real tx construction: lines 177–211 (placeholder Instruction at lines 195–199; tx send at 200–203).
	- sell path and TODO for real tx: lines 213–241 (placeholder Instruction at lines 230–234; tx send at 235–238).

- `src/ws.rs`
	- logs subscribe: lines 28–37 (logsSubscribe payload built at 31–36).
	- account subscribe for holdings: lines 39–71 (PDA generation at 46–56).
	- incoming message handling and subscription caveat: lines 73–106 (lossy sub-id -> mint mapping comment at lines 96–99).

- `src/models.rs` — `BondingCurveState` definition used to parse on-chain curve state: lines 8–17; `Holding` and `PriceCache`: lines 19–26.

## How to implement real transactions — short checklist
Follow this checklist when implementing buy/sell transaction construction. These are low-level steps and must be reviewed manually before enabling `--real`.

1. Review RPC client choice (currently `RpcClient::new(&settings.solana_rpc_urls[0])`) — consider multi-endpoint fallback for sending txs.
2. Construct correct `Instruction`s for pump.fun `buy` and `sell` using the Anchor IDL in `pumpfun.json` / `pumpfunamm.json`:
	 - Use the account ordering from the IDL (see `pumpfun.json` `instructions.buy.accounts`) and the correct discriminator in `data`.
3. Add payer, mint, bonding curve PDA, associated token accounts, and any required signer PDAs to `Instruction.accounts` (see TODO notes at `src/rpc.rs` lines 195–199 and 230–234).
4. Build and partially sign the transaction, then simulate locally (RPC `simulateTransaction`) to verify accounts and semantics before calling `send_and_confirm_transaction`.
5. Add proper error handling and idempotency (avoid double buys on the same signature). Consider persistence of `seen` and holdings across restarts.
6. Secure key handling: never commit `wallet_keypair_path` content to source control. Prompt for human review before enabling `--real` in CI.

---

If you want, I can now:
- Add `config.example.toml` to the repo (with placeholders) — I can create it now.
- Add a small README snippet showing how to copy `config.example.toml` -> `config.toml` and run the program in dry vs real mode.

Tell me which of the two you'd like next and I'll apply the change.

## Exact TODOs & lines to inspect
If you're about to implement real transactions or change detection logic, inspect these exact lines first (they contain the most sensitive code paths and TODO markers):

- `src/main.rs`:
	- detection log string: line 113
	- buy trigger and hard-coded 0.1 SOL: line 137
	- holdings monitor sell invocation: line 188

- `src/rpc.rs`:
	- transaction parsing (creator & mint extraction): lines 31–73
	- `fetch_token_metadata`: lines 75–99
	- `fetch_with_fallback` (RPC fallback): lines 101–127
	- `fetch_current_price` (bonding curve parsing): lines 129–175
	- `buy_token` TODO placeholders: lines 192–203 (see TODO comments at 195–199)
	- `sell_token` TODO placeholders: lines 227–239 (see TODO comments at 230–234)

- `src/ws.rs`:
	- logsSubscribe payload: lines 31–36
	- accountSubscribe and PDA creation: lines 46–56
	- subscription ID -> mint caveat comment: lines 96–99

These are the smallest, highest-risk changes — please request a review if you modify any of them.
