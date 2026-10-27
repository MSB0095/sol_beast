# RPC providers: configuration and free-tier assessment

Free Solana RPC is the binding constraint on a 24/7 keeper. Two independent
limits fail in different ways, and both are configured per endpoint:

1. **requests per second** — exceeding it returns 429 and, on some providers,
   a connection-level block;
2. **requests per month** — a long-running feed can exhaust a free quota in
   days even while staying far under the RPS limit.

The engine gives every endpoint its own `max_rps`, `heavy_rps` (a tighter limit
for methods free tiers throttle hardest), `monthly_call_budget`, `role` and
`priority`, and refuses to issue a request it knows will be rejected.

## Verified free-tier numbers

Every provider below was probed with a real `getHealth` call on 2026-10-26/27.
A published limit was not enough: dRPC advertises a free tier that does not
actually serve Solana.

| Provider | Free volume | RPS | `getProgramAccounts` | `getTokenAccountsByOwner` | Verified | Verdict |
|---|---|---|---|---|---|---|
| **Helius** | 1M credits/month | 10 (GPA 5/s, send 1/s) | yes, with filters | yes | `getHealth: ok`, 630 KLend reserves decoded | **exec + discovery + rent** |
| **Alchemy** | 30M CU (~1.1M requests) | 25 | yes | yes | `getHealth: ok` | **fallback** |
| **Public** | none | 4 per method | yes, slow | yes | `getHealth: ok` | **feed** — no cost, no SLA |
| **dRPC** | advertised 210M CU | — | — | — | `solana.drpc.org` returns `code 35`: *"chain is not available on free plan, please upgrade to paid plan"* | **not free**; paid key only |
| Chainstack | 3M RU | 25 | **blocked on Developer** | likely blocked | docs | out |
| Ankr | 200M credits ÷ 500 | ~30 | yes | yes | **~16h history only** | out |
| QuickNode | 10M credits, 30-day trial | 15 | yes | yes | trial expires | out |
| RPC Fast | 1.5M CU | 15 | **all heavy methods blocked** | ✗ | docs | out |
| BlockPI | 50M RU | 20 | — | — | Solana only on $299 | out |

**dRPC needs a paid plan for Solana.** The free preset was removed from
`beast rpc add`; do not configure `https://solana.drpc.org` expecting it to
work. A paid dRPC account can still be added manually, and `drpc_api_key` is
supported in `secrets.toml`, but neither is recommended for a zero-cost setup.

**Mainnet gRPC / Yellowstone / LaserStream / Shredstream: not available on any
free tier.** The engine's feed is pull-based (`getMultipleAccounts`), so this
does not block radar, discovery, settlement or execution.

## What sol_beast actually calls

| Role | Methods | Volume |
|---|---|---|
| `feed` | `getMultipleAccounts` ×3 per tick (all pools, all configs, all vaults/mints) | highest |
| `exec` | `getBalance`, `getAccountInfo`, `getLatestBlockhash`, `simulateTransaction`, `sendTransaction` | low, latency-critical |
| `settlement` | `getTransaction`, `getSignatureStatuses` (routed to `exec`) | ~4 per landed bundle |
| `discovery` | `getProgramAccounts` (KLend reserve scans, pool seeding) | occasional, heavy |
| `rent` | `getTokenAccountsByOwner` ×2 | rare |

Jito bundle submission is a separate block-engine host and is unaffected by RPC
quotas.

### The batched feed

The feed used to issue 2–3 calls **per pool per tick**: 10 pools meant 20–30
calls every interval, which burns a 1M-credit monthly quota in about a week.
`refresh_all_batched` collapses one tick to **three** `getMultipleAccounts`
calls regardless of watchlist size.

| Feed strategy | Calls/tick @10 pools | RPS | Calls/month @24/7 |
|---|---|---|---|
| Before (per pool) | 20–30 | 10–15 | ~26M–39M |
| After (batched) | 3 | 1.5 | ~3.9M |

Even 3.9M calls/month is beyond Helius (1M) and Alchemy (~1.1M), so the
recommended split sends the high-volume feed to the public endpoint and keeps
the keyed free tiers for the low-volume, latency-sensitive and heavy calls.
This is the configuration that survives indefinitely at zero cost:

| Endpoint | Role | Why |
|---|---|---|
| Helius (`helius-exec`) | exec | Fast blockhash/simulate/send; 1M credits is plenty for execution traffic. |
| Helius (`helius-discovery`) | discovery | Filtered `getProgramAccounts` is enabled and returns all 630 KLend reserves. |
| Helius (`helius-rent`) | rent | `getTokenAccountsByOwner` is enabled; rent scans are rare. |
| Alchemy (`alchemy-fallback`) | fallback | 30M CU absorbs bursts or a Helius outage. |
| Public (`public-feed`) | feed | No key, no monthly budget; 4 RPS is enough for a 3-call batched tick. |
| Public (`public-fallback`) | fallback | Last resort at lower priority than Alchemy. |

Three separate Helius endpoint entries can share one key: each entry has its
own bucket and budget, which matches how the role split is used. The budgets
shown below total less than the 1M credit allowance on purpose.

## Recommended configuration

```toml
[[rpc.endpoints]]
name = "helius-exec"
url = "https://mainnet.helius-rpc.com/?api-key=${HELIUS_API_KEY}"
role = "exec"
max_rps = 8
heavy_rps = 4
monthly_call_budget = 300000
priority = 1
```

Repeat for `discovery` (budget 300000) and `rent` (budget 100000), then add:

```toml
[[rpc.endpoints]]
name = "alchemy-fallback"
url = "https://solana-mainnet.g.alchemy.com/v2/${ALCHEMY_API_KEY}"
role = "fallback"
max_rps = 12
heavy_rps = 4
monthly_call_budget = 900000
priority = 10

[[rpc.endpoints]]
name = "public-feed"
url = "https://api.mainnet-beta.solana.com"
role = "feed"
max_rps = 3
heavy_rps = 1
monthly_call_budget = 0
priority = 10
```

`${...}` references are expanded from the environment and from the keys in
`secrets.toml`; a missing key is a startup error, never a silent empty string.
A complete tracked version of this split is
[`config.v2.example.toml`](../config.v2.example.toml). The CLI can append
presets:

```bash
beast rpc add --provider helius   --role exec      --key-env HELIUS_API_KEY
beast rpc add --provider helius   --role discovery --key-env HELIUS_API_KEY
beast rpc add --provider helius   --role rent      --key-env HELIUS_API_KEY
beast rpc add --provider alchemy  --role fallback  --key-env ALCHEMY_API_KEY
beast rpc add --provider public   --role feed
```

Then inspect and probe:

```bash
beast rpc list                 # name / role / priority / limits / url
beast doctor                   # per-endpoint health + capability probes
```

`beast doctor` reports, per endpoint, role, round-trip time and current slot,
plus two capability probes: `cap:getProgramAccounts` (filtered KLend scan) and
`cap:getTokenAccountsByOwner`. Those are the methods free tiers actually
disable, so a red capability line means discovery or rent reclamation is
unavailable on that endpoint — and the engine routes around it rather than
failing mid-run.

## Role routing

Every RPC method maps to one role:

| Method | Role |
|---|---|
| `getMultipleAccounts` | `feed` |
| `getBalance`, `getAccountInfo`, `getLatestBlockhash`, `isBlockhashValid`, `getSignatureStatuses`, `getTransaction`, `simulateTransaction`, `sendTransaction`, `sendRawTransaction` | `exec` |
| `getProgramAccounts`, `getTokenLargestAccounts` | `discovery` |
| `getTokenAccountsByOwner`, `getTokenAccountsByDelegate` | `rent` |
| anything else | `feed` |

Selection is strict about preference and loose about survival:

1. an endpoint whose configured role matches the call, lowest `priority` first;
2. otherwise a `fallback` endpoint, lowest `priority` first;
3. otherwise any healthy endpoint.

That last step means a misconfigured role can never make the process look dead;
it only loses the latency/limit benefit of the split. A 429, `-32005` or
`-32429` opens that endpoint's circuit for `rpc.circuit_breaker_seconds` while
the others keep serving.

## Call budgets

`monthly_call_budget` is **process-local accounting**, not provider-side
enforcement. The client counts calls per endpoint and stops selecting an
endpoint after the configured budget. The window starts on first use and resets
after 30 days of continuous process uptime; restarting the process resets it.
Treat it as a safety margin below the provider's published quota, not as an
authoritative meter.

Use `0` for endpoints with no meaningful monthly quota (the public endpoint).
`GET /v1/status` exposes `rpc_endpoints` with per-endpoint calls, budget usage,
priority, role, rate limits and circuit state.

## Sources

- Helius rate limits: https://www.helius.dev/docs/billing/rate-limits
- Alchemy Solana overview (per-method CU): https://www.alchemy.com/overviews/solana-rpc
- dRPC rate limiting: https://drpc.org/docs/howitworks/ratelimiting
- Solana public endpoint limits: https://solana.com/docs/references/clusters
- Chainstack free Developer plan / GPA: https://chainstack.com/best-free-tier-rpcs-web3-builders-2026/
- RPC Fast free-endpoint comparison: https://rpcfast.com/blog/free-solana-rpc-endpoints

## Limitations

- gRPC/streaming is unused; the feed is pull-based by design. Helius REST/WSS
  works; the earlier gRPC feed path was removed because free gRPC is not
  available.
- Jito bundle submission is not an RPC call and is unaffected by these quotas.
- Multi-key rotation across several free accounts is not implemented; one key
  per provider per role is the supported model.
- A process restart resets the local budget counters even though the provider's
  monthly quota does not.
