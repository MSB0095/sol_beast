# RPC providers: free-tier assessment and configuration

All numbers below were re-verified against each provider's own docs (see
"Sources"). Free tiers fail in two independent ways, and both must be
configured per endpoint:

1. **requests per second** — exceeding it returns 429 and, on some providers,
   a connection-level block;
2. **requests per month** — a long-running feed can exhaust a free quota in
   days even while staying far under the RPS limit.

The engine therefore gives every endpoint its own `max_rps`, `heavy_rps`
(separate limit for the methods free tiers throttle hardest) and
`monthly_call_budget`, and refuses to issue a request it knows will be
rejected.

## Verified free-tier numbers

| Provider | Free volume | Effective calls/mo | RPS | `getProgramAccounts` | `getTokenAccountsByOwner` | Archival | Verdict |
|---|---|---|---|---|---|---|---|
| **dRPC** | 210M CU / 30d | ~10.5M | ~100 | yes | yes | full (aggregated) | **feed** — only tier with real headroom |
| **Helius** | 1M credits | 1M | 10 (GPA 5/s, sendTx 1/s) | yes | yes | yes | **exec + discovery + rent** |
| **Alchemy** | 30M CU (~1.1M req @ ~27 CU avg) | ~1.1M | 25 | yes | yes | on free | **fallback** |
| **Public** | none | unlimited but throttled | 10 IP / 4 per method | yes (slow) | yes | none | last resort, no SLA |
| Chainstack | 3M RU | 3M | 25 (their blog) / 5 (QuickNode's) | **blocked on Developer** | likely blocked | 2 RU | out |
| Ankr | 200M credits ÷ 500 = 400k req | ~400k | ~30 (their page says 30 req/min) | yes | yes | **~16h only** | out |
| QuickNode | 10M credits, **30-day trial** | ~333k | 15 | yes | yes | yes | out — not permanent |
| RPC Fast | 1.5M CU | ~330k | 15 | **all 4 heavy methods blocked** | ✗ | — | out |
| BlockPI | 50M RU | — | 20 | — | — | Solana only on $299 | out |

**Mainnet gRPC / Yellowstone / LaserStream / Shredstream: not available on any
free tier.** The engine's feed is pull-based (`getMultipleAccounts`), so this
does not block radar, discovery, settlement or execution.

## What sol_beast actually calls

| Role | Methods | Volume |
|---|---|---|
| feed | `getMultipleAccounts` ×3 per tick (all pools, all configs, all vaults/mints) | see below |
| exec | `getBalance`, `getLatestBlockhash` | ~2 per prepared cycle |
| settlement | `getTransaction`, `getSignatureStatuses` | ~4 per landed bundle |
| discovery | `getProgramAccounts` | occasional |
| rent | `getTokenAccountsByOwner` ×2 | rare |
| Jito | sendBundle / simulateBundle / getBundleStatuses | separate host, not RPC |

### The call budget that decides everything

The feed used to issue 2–3 calls **per pool per tick**: 10 pools meant 20–30
calls every interval, which burns a 1M-credit monthly quota in about a week.
`refresh_all_batched` collapses one tick to **three** `getMultipleAccounts`
calls regardless of watchlist size.

| Feed strategy | Calls/tick @10 pools | RPS | Calls/month @24/7 |
|---|---|---|---|
| Before (per pool) | 20–30 | 10–15 | ~26M–39M |
| After (batched) | 3 | 1.5 | ~3.9M |

Against the free tiers above, only **dRPC** survives a 24/7 batched feed
(~37% of budget); Helius and Alchemy last ~8 days, so they belong on the
low-volume exec path where their latency matters most.

## Recommended configuration

```bash
export DRPC_KEY=...          # https://drpc.org
export HELIUS_API_KEY=...    # https://dashboard.helius.dev/signup?plan=free
beast rpc add --provider drpc      --role feed      --key-env DRPC_KEY
beast rpc add --provider helius    --role exec      --key-env HELIUS_API_KEY
beast rpc add --provider helius    --role discovery --key-env HELIUS_API_KEY
beast rpc add --provider helius    --role rent      --key-env HELIUS_API_KEY
beast rpc add --provider alchemy   --role fallback  --key-env ALCHEMY_API_KEY
```

Then inspect and probe:

```bash
beast rpc list                 # name / role / limits / url
beast doctor                   # per-endpoint health + capability probes
```

`beast doctor` reports, per endpoint, role, round-trip time and current slot,
plus two capability probes: `cap:getProgramAccounts` and
`cap:getTokenAccountsByOwner`. Those two are the methods free tiers actually
disable, so a red capability line means discovery or rent reclamation is
unavailable on that endpoint — and the engine routes around it rather than
failing mid-run.

## Rate limiting and circuit breaking

Every endpoint has a token bucket sized from its own published limit, and heavy
methods get a second, tighter bucket (for example Helius free allows 10 RPS
overall but 5 RPS for `getProgramAccounts`). A 429, `-32005` or `-32429` opens
that endpoint's circuit for `rpc.circuit_breaker_seconds` while the others keep
serving. `GET /v1/status` exposes `rpc_endpoints` with per-endpoint calls, rate
limits and circuit state.

## Sources

- dRPC rate limiting: https://drpc.org/docs/howitworks/ratelimiting
- Helius rate limits: https://www.helius.dev/docs/billing/rate-limits
- Alchemy Solana overview (per-method CU): https://www.alchemy.com/overviews/solana-rpc
- Chainstack free Developer plan / GPA: https://chainstack.com/best-free-tier-rpcs-web3-builders-2026/
- Solana public endpoint limits: https://solana.com/docs/references/clusters
- RPC Fast free-endpoint comparison: https://rpcfast.com/blog/free-solana-rpc-endpoints

## Limitations

- gRPC/streaming is untested and unused; the feed is pull-based by design.
- Jito bundle submission is not an RPC call and is unaffected by these quotas.
- Multi-endpoint key rotation across several free accounts is not implemented;
  one key per provider per role is the supported model.
