//! Non-daemon CLI commands.

use anyhow::{bail, Context, Result};
use beast_core::config::{
    self, BeastConfig, EndpointRole, GuardMode, LoadedConfig, RpcEndpointConfig,
    DEFAULT_CONFIG_TOML,
};
use beast_dex::catalog::{self, VenueKind, VENUES};
use beast_dex::jupiter::{JupiterClient, QuoteRequest};
use beast_dex::Address;
use beast_exec::{RentReclaimer, Wallet};
use beast_feed::{refresh_pool, WatchedPool};
use beast_rpc::{EndpointOptions, RpcClient};
use beast_store::Store;
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

fn load(path: Option<&Path>, real: bool) -> Result<LoadedConfig> {
    let loaded = config::load(path, real)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(loaded)
}

fn rpc(config: &BeastConfig) -> Result<RpcClient> {
    Ok(RpcClient::new(
        config.rpc.urls.clone(),
        config.rpc.commitment.clone(),
        config.rpc.timeout_ms,
        config.rpc.max_retries,
    )?)
}

fn jupiter(config: &BeastConfig, secrets: &config::Secrets) -> Result<JupiterClient> {
    let base = if config.rpc.jupiter_quote_url.trim().is_empty() {
        "https://lite-api.jup.ag/swap/v1".to_string()
    } else {
        config.rpc.jupiter_quote_url.clone()
    };
    Ok(JupiterClient::with_base_url(
        base,
        secrets.jupiter_api_key.clone(),
    )?)
}

fn guard_mode_name(mode: GuardMode) -> &'static str {
    match mode {
        GuardMode::Lite => "lite",
        GuardMode::Program => "program",
    }
}

pub fn check(path: Option<&Path>, real: bool, json_output: bool) -> Result<()> {
    let loaded = load(path, real)?;
    let config = &loaded.config;
    let effective = if real && config.runtime.real {
        config.runtime.mode
    } else {
        config
            .runtime
            .mode
            .capped_by(beast_core::types::Mode::Radar)
    };
    let live_strategies = config
        .strategies
        .iter()
        .filter(|(_, strategy)| strategy.mode.is_live())
        .count();
    let radar_strategies = config
        .strategies
        .iter()
        .filter(|(_, strategy)| strategy.mode == beast_core::types::Mode::Radar)
        .count();

    let guard_program_ok = !config.guard.program_id.is_empty()
        && config
            .guard
            .program_id
            .parse::<solana_sdk::pubkey::Pubkey>()
            .is_ok();
    let wallet_present = config
        .wallet
        .keypair_path
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false)
        || loaded.secrets.wallet_keypair_json.is_some();
    let live_ready = guard_program_ok
        && config.guard.mode == GuardMode::Program
        && config.jito.enabled
        && wallet_present;

    if json_output {
        let value = json!({
            "config_path": loaded.config_path,
            "secrets_path": loaded.secrets_path(),
            "generated_secrets": loaded.generated_secrets,
            "warnings": loaded.warnings,
            "mode": config.runtime.mode.as_str(),
            "effective_mode": effective.as_str(),
            "real": config.runtime.real,
            "strategies": {
                "total": config.strategies.len(),
                "radar": radar_strategies,
                "live": live_strategies,
            },
            "pools_configured": config.pools.len(),
            "venues_registered": VENUES.len(),
            "guard": {
                "enabled": config.guard.enabled,
                "mode": guard_mode_name(config.guard.mode),
                "program_id": config.guard.program_id,
                "program_id_valid": guard_program_ok,
                "min_profit_lamports": config.guard.min_profit_lamports,
            },
            "execution": {
                "jito_enabled": config.jito.enabled,
                "wallet_present": wallet_present,
                "live_ready": live_ready,
            },
            "api": {
                "enabled": config.api.enabled,
                "bind": config.api.bind,
                "allowed_origins": config.api.allowed_origins,
                "tokens_present": loaded.secrets.api_bearer_token.is_some()
                    && loaded.secrets.api_read_token.is_some(),
            },
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("config:        {:?}", loaded.config_path);
        println!("secrets:       {:?}", loaded.secrets_path());
        println!(
            "mode:          {} (effective {})",
            config.runtime.mode, effective
        );
        println!("real:          {}", config.runtime.real);
        println!(
            "strategies:    {} configured, {radar_strategies} radar, {live_strategies} live",
            config.strategies.len()
        );
        println!("pools:         {} configured", config.pools.len());
        println!("venues:        {} registered", VENUES.len());
        println!(
            "guard:         {} / {} / valid={guard_program_ok}",
            config.guard.enabled,
            guard_mode_name(config.guard.mode)
        );
        println!("jito:          {}", config.jito.enabled);
        println!(
            "wallet:        {}",
            if wallet_present { "present" } else { "missing" }
        );
        println!(
            "api:           {} @ {}",
            config.api.enabled, config.api.bind
        );
        if effective.is_live() && !live_ready {
            println!();
            println!(
                "LIVE IS NOT READY: guard=program, program_id, Jito and wallet are all required."
            );
        }
        if loaded.generated_secrets {
            println!();
            println!("API tokens were generated in the secrets file; keep it mode 0600.");
        }
        for warning in &loaded.warnings {
            println!("warning: {warning}");
        }
    }
    Ok(())
}

pub fn venues(json_output: bool, kind: Option<&str>) -> Result<()> {
    let kind = kind
        .map(|value| value.parse::<VenueKind>())
        .transpose()
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let selected: Vec<_> = VENUES
        .iter()
        .filter(|venue| kind.map(|kind| kind == venue.kind).unwrap_or(true))
        .collect();
    if json_output {
        let list: Vec<Value> = selected
            .iter()
            .map(|venue| {
                json!({
                    "id": venue.id,
                    "name": venue.name,
                    "program_id": venue.program_id,
                    "kind": venue.kind,
                    "coverage": venue.coverage.as_str(),
                    "decoder": venue.decoder,
                    "math": venue.math,
                    "docs": venue.docs,
                    "notes": venue.notes,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "count": list.len(),
                "venues": list,
            }))?
        );
    } else {
        println!(
            "{:<28} {:<18} {:<16} {:<10} PROGRAM",
            "ID", "KIND", "COVERAGE", "DECODER"
        );
        for venue in selected {
            println!(
                "{:<28} {:<18} {:<16} {:<10} {}",
                venue.id,
                venue.kind.as_str(),
                venue.coverage.as_str(),
                venue
                    .decoder
                    .map(|decoder| format!("{decoder:?}"))
                    .unwrap_or_else(|| "-".into()),
                venue.program_id
            );
        }
    }
    Ok(())
}

pub async fn discover(
    path: Option<&Path>,
    venue_id: &str,
    limit: usize,
    save: bool,
    json_output: bool,
) -> Result<()> {
    let loaded = load(path, false)?;
    let config = &loaded.config;
    let venue =
        catalog::venue(venue_id).ok_or_else(|| anyhow::anyhow!("unknown venue {venue_id}"))?;
    if venue.decoder.is_none() {
        bail!(
            "venue {} has no account decoder; use Jupiter quote-only mode",
            venue.id
        );
    }
    if limit == 0 || limit > 1_000 {
        bail!("--limit must be 1..=1000");
    }
    let rpc = rpc(config)?;
    let program_id = venue
        .program_id
        .parse::<Address>()
        .map_err(|error| anyhow::anyhow!("venue program id: {error}"))?;
    println!("discovering {} ({}) ...", venue.id, venue.program_id);
    let accounts = rpc
        .get_program_accounts(program_id, Vec::new(), Some((0, 0)))
        .await
        .with_context(|| format!("getProgramAccounts {}", venue.program_id))?;
    println!(
        "received {} accounts; decoding up to {limit}",
        accounts.len()
    );

    let store = if save {
        Some(Store::open(&config.storage.path)?)
    } else {
        None
    };
    let mut pools = Vec::new();
    let mut errors = Vec::new();
    let mut attempted = 0usize;
    // Programs can expose non-pool accounts. Keep walking until the requested
    // number of pools decodes, but cap wasted RPC calls.
    let max_attempts = accounts.len().min(limit.saturating_mul(10).max(limit));
    for (address, _account) in accounts.into_iter().take(max_attempts) {
        if pools.len() >= limit {
            break;
        }
        attempted += 1;
        let watched = WatchedPool::from_parts(address, venue.id);
        match refresh_pool(&rpc, &watched, true).await {
            Ok(snapshot) => {
                if let Some(store) = &store {
                    if let Err(error) = store.upsert_pool(&snapshot) {
                        errors.push(format!("{address}: store: {error}"));
                        continue;
                    }
                }
                pools.push(snapshot);
            }
            Err(error) => errors.push(format!("{address}: {error}")),
        }
    }
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "venue": venue.id,
                "program_id": venue.program_id,
                "scanned": attempted,
                "decoded": pools.len(),
                "saved": save,
                "pools": pools,
                "errors": errors,
            }))?
        );
    } else {
        for pool in &pools {
            println!(
                "{} {} {} / {} slot={} confidence={:.2} model={}",
                pool.address,
                pool.venue,
                pool.mint_a,
                pool.mint_b,
                pool.slot,
                pool.confidence,
                serde_json::to_string(&pool.model).unwrap_or_else(|_| "?".into())
            );
        }
        println!("decoded {} pools; {} errors", pools.len(), errors.len());
        for error in errors.iter().take(20) {
            println!("  {error}");
        }
        if save {
            println!("saved to {}", config.storage.path.display());
        }
    }
    Ok(())
}

pub async fn quote(
    path: Option<&Path>,
    input_mint: &str,
    output_mint: &str,
    amount: u64,
    slippage_bps: u16,
) -> Result<()> {
    let loaded = load(path, false)?;
    let input = input_mint
        .parse::<Address>()
        .map_err(|error| anyhow::anyhow!("input mint: {error}"))?;
    let output = output_mint
        .parse::<Address>()
        .map_err(|error| anyhow::anyhow!("output mint: {error}"))?;
    if amount == 0 {
        bail!("--amount must be > 0");
    }
    let client = jupiter(&loaded.config, &loaded.secrets)?;
    let mut request = QuoteRequest::new(input, output, amount);
    request.slippage_bps = slippage_bps.clamp(1, 500);
    let quote = client.quote(&request).await?;
    println!("{}", serde_json::to_string_pretty(&quote)?);
    Ok(())
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: &'static str,
    ok: bool,
    detail: String,
}

/// One heavy-method probe against a dedicated endpoint for its role.
struct CapabilityProbe {
    name: &'static str,
    ok: bool,
    detail: String,
}

/// Probe the heavy methods free tiers actually block.
///
/// `getProgramAccounts` powers pool discovery and `getTokenAccountsByOwner`
/// powers rent reclamation. Both are commonly disabled or throttled on free
/// plans, and a failure only surfaces mid-run, so they get probed up front.
async fn probe_capabilities(config: &BeastConfig) -> Vec<CapabilityProbe> {
    let endpoints = config.rpc.effective_endpoints();
    let by_role = |role: beast_core::config::EndpointRole| -> Option<&RpcEndpointConfig> {
        endpoints
            .iter()
            .find(|endpoint| endpoint.role == role)
            .or_else(|| endpoints.first())
    };

    let client_for = |endpoint: &RpcEndpointConfig| {
        beast_rpc::RpcClient::with_endpoints(
            vec![
                EndpointOptions::new(endpoint.name.clone(), endpoint.url.clone())
                    .with_role_name(endpoint.role.as_str())
                    .with_rps(endpoint.max_rps, endpoint.heavy_rps)
                    .with_budget(endpoint.monthly_call_budget)
                    .with_priority(endpoint.priority),
            ],
            config.rpc.commitment.clone(),
            config.rpc.timeout_ms,
            0,
            0,
        )
    };

    let mut probes = Vec::new();

    // Probe the exact call the flash runtime makes: KLend reserves filtered by
    // their 8624-byte account size. Filtering by an unfiltered token program
    // requests millions of accounts and modern providers reject it outright,
    // which would report a false capability failure.
    let discovery = by_role(EndpointRole::Discovery).or_else(|| by_role(EndpointRole::Fallback));
    if let Some(endpoint) = discovery {
        let program = "KLend2g3cP87fffoy8q1mQqGKjrxjC8boSyAYavgmjD"
            .parse::<beast_dex::Address>()
            .expect("KLend program id");
        let result = client_for(endpoint);
        probes.push(match result {
            Ok(client) => {
                let started = std::time::Instant::now();
                match client
                    .get_program_accounts(
                        program,
                        vec![serde_json::json!({"dataSize": 8624})],
                        Some((0, 0)),
                    )
                    .await
                {
                    Ok(accounts) => CapabilityProbe {
                        name: "cap:getProgramAccounts",
                        ok: true,
                        detail: format!(
                            "{} accounts via {} in {}ms",
                            accounts.len(),
                            endpoint.name,
                            started.elapsed().as_millis()
                        ),
                    },
                    Err(error) => CapabilityProbe {
                        name: "cap:getProgramAccounts",
                        ok: false,
                        detail: format!("{} blocked: {error}", endpoint.name),
                    },
                }
            }
            Err(error) => CapabilityProbe {
                name: "cap:getProgramAccounts",
                ok: false,
                detail: error.to_string(),
            },
        });
    } else {
        probes.push(CapabilityProbe {
            name: "cap:getProgramAccounts",
            ok: false,
            detail: "no discovery-capable endpoint configured".into(),
        });
    }

    // getTokenAccountsByOwner against a known non-empty wallet so the call
    // returns a small, deterministic payload.
    let rent = by_role(EndpointRole::Rent).or_else(|| by_role(EndpointRole::Fallback));
    if let Some(endpoint) = rent {
        // A wallet with no token accounts at the time of writing, so the
        // probe stays small and deterministic; it is only used to test method
        // availability, never to sign.
        let owner = "EtW8SkG2fFiLWzLpuFawvaESLTtn4qkQKvGa8cyBfV6x"
            .parse::<beast_dex::Address>()
            .expect("probe owner");
        let token_program = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
            .parse::<beast_dex::Address>()
            .expect("token program id");
        let result = client_for(endpoint);
        probes.push(match result {
            Ok(client) => {
                let started = std::time::Instant::now();
                match client
                    .get_token_accounts_by_owner(owner, token_program)
                    .await
                {
                    Ok(accounts) => CapabilityProbe {
                        name: "cap:getTokenAccountsByOwner",
                        ok: true,
                        detail: format!(
                            "{} accounts via {} in {}ms",
                            accounts.len(),
                            endpoint.name,
                            started.elapsed().as_millis()
                        ),
                    },
                    Err(error) => CapabilityProbe {
                        name: "cap:getTokenAccountsByOwner",
                        ok: false,
                        detail: format!("{} blocked: {error}", endpoint.name),
                    },
                }
            }
            Err(error) => CapabilityProbe {
                name: "cap:getTokenAccountsByOwner",
                ok: false,
                detail: error.to_string(),
            },
        });
    } else {
        probes.push(CapabilityProbe {
            name: "cap:getTokenAccountsByOwner",
            ok: false,
            detail: "no rent-capable endpoint configured".into(),
        });
    }

    probes
}

pub async fn doctor(path: Option<&Path>, json_output: bool, no_network: bool) -> Result<()> {
    let loaded = load(path, false)?;
    let config = &loaded.config;
    let mut checks = Vec::new();

    checks.push(DoctorCheck {
        name: "config",
        ok: true,
        detail: format!(
            "mode={} real={} warnings={}",
            config.runtime.mode,
            config.runtime.real,
            loaded.warnings.len()
        ),
    });

    let store_path = config.storage.path.clone();
    let store_check = Store::open(&store_path).and_then(|store| {
        let result = store.quick_check()?;
        Ok(result)
    });
    checks.push(DoctorCheck {
        name: "store",
        ok: store_check.as_deref().map(|v| v == "ok").unwrap_or(false),
        detail: match &store_check {
            Ok(value) => format!("{}: {value}", store_path.display()),
            Err(error) => format!("{}: {error}", store_path.display()),
        },
    });

    let wallet = if let Some(path) = config
        .wallet
        .keypair_path
        .as_ref()
        .filter(|path| path.exists())
    {
        Some(beast_exec::Wallet::load(path, config.wallet.require_0600))
    } else {
        loaded
            .secrets
            .wallet_keypair_json
            .as_deref()
            .map(beast_exec::Wallet::from_json)
    };
    let wallet_address = wallet
        .as_ref()
        .and_then(|wallet| wallet.as_ref().ok())
        .map(|wallet| wallet.address());
    checks.push(DoctorCheck {
        name: "wallet",
        ok: wallet.is_some(),
        detail: match (&wallet, wallet_address) {
            (Some(Ok(_)), Some(address)) => address.to_string(),
            (Some(Err(error)), _) => error.to_string(),
            _ => "not configured (fine for radar)".into(),
        },
    });

    let guard_program_ok = !config.guard.program_id.is_empty()
        && config
            .guard
            .program_id
            .parse::<solana_sdk::pubkey::Pubkey>()
            .is_ok();
    checks.push(DoctorCheck {
        name: "guard",
        ok: if config.guard.mode == GuardMode::Program {
            guard_program_ok
        } else {
            config.guard.enabled
        },
        detail: format!(
            "mode={} program_id={} min_profit={}",
            guard_mode_name(config.guard.mode),
            if config.guard.program_id.is_empty() {
                "<unset>"
            } else {
                config.guard.program_id.as_str()
            },
            config.guard.min_profit_lamports
        ),
    });

    if no_network {
        checks.push(DoctorCheck {
            name: "rpc",
            ok: true,
            detail: "skipped (--no-network)".into(),
        });
        checks.push(DoctorCheck {
            name: "jupiter",
            ok: true,
            detail: "skipped (--no-network)".into(),
        });
    } else {
        let rpc = rpc(config)?;

        // Probe each configured endpoint independently. Free tiers disagree on
        // which heavy methods they allow, so a global health check would hide a
        // dead discovery/rent path behind a healthy feed.
        for endpoint in config.rpc.effective_endpoints() {
            let started = std::time::Instant::now();
            let client = beast_rpc::RpcClient::with_endpoints(
                vec![
                    EndpointOptions::new(endpoint.name.clone(), endpoint.url.clone())
                        .with_role_name(endpoint.role.as_str())
                        .with_rps(endpoint.max_rps, endpoint.heavy_rps)
                        .with_budget(endpoint.monthly_call_budget)
                        .with_priority(endpoint.priority),
                ],
                config.rpc.commitment.clone(),
                config.rpc.timeout_ms,
                1,
                0,
            )?;
            let health = client.get_health().await;
            let slot = client.get_slot().await;
            let elapsed_ms = started.elapsed().as_millis();
            let ok = health
                .as_deref()
                .map(|value| value == "ok")
                .unwrap_or(false)
                && slot.is_ok();
            let detail = match (&health, &slot) {
                (Ok(health), Ok(slot)) => {
                    format!(
                        "role={} health={health} slot={slot} rtt={elapsed_ms}ms max_rps={} heavy_rps={}",
                        endpoint.role.as_str(),
                        endpoint.max_rps,
                        endpoint.heavy_rps
                    )
                }
                (Ok(health), Err(error)) => {
                    format!(
                        "role={} health={health} slot error: {error}",
                        endpoint.role.as_str()
                    )
                }
                (Err(error), _) => {
                    format!("role={} unreachable: {error}", endpoint.role.as_str())
                }
            };
            checks.push(DoctorCheck {
                name: Box::leak(format!("rpc:{}", endpoint.name).into_boxed_str()),
                ok,
                detail,
            });
            let _ = client;
        }

        // Capability probes for the methods the bot actually depends on.
        let capabilities = probe_capabilities(config).await;
        for capability in capabilities {
            checks.push(DoctorCheck {
                name: capability.name,
                ok: capability.ok,
                detail: capability.detail,
            });
        }

        let client = jupiter(config, &loaded.secrets)?;
        let mut request = QuoteRequest::new(
            "So11111111111111111111111111111111111111112"
                .parse::<Address>()
                .expect("WSOL"),
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"
                .parse::<Address>()
                .expect("USDC"),
            1_000_000,
        );
        request.slippage_bps = 50;
        match client.quote(&request).await {
            Ok(quote) => checks.push(DoctorCheck {
                name: "jupiter",
                ok: quote.amount_out().unwrap_or(0) > 0,
                detail: format!(
                    "1_000_000 lamports -> {} USDC raw (slot {})",
                    quote.amount_out().unwrap_or(0),
                    quote.context_slot
                ),
            }),
            Err(error) => checks.push(DoctorCheck {
                name: "jupiter",
                ok: false,
                detail: error.to_string(),
            }),
        }

        if let Some(Ok(wallet)) = &wallet {
            match rpc.get_balance(wallet.address()).await {
                Ok(balance) => checks.push(DoctorCheck {
                    name: "balance",
                    ok: balance >= config.risk.balance_floor_lamports,
                    detail: format!(
                        "{balance} lamports (floor {})",
                        config.risk.balance_floor_lamports
                    ),
                }),
                Err(error) => checks.push(DoctorCheck {
                    name: "balance",
                    ok: false,
                    detail: error.to_string(),
                }),
            }
        }
    }

    if config.guard.mode == GuardMode::Program && guard_program_ok && !no_network {
        if let Ok(rpc) = rpc(config) {
            if let Ok(program_id) = config.guard.program_id.parse::<Address>() {
                match rpc.get_account_info(program_id).await {
                    Ok(Some(account)) => checks.push(DoctorCheck {
                        name: "guard_account",
                        ok: account.account.executable,
                        detail: format!(
                            "executable={} owner={}",
                            account.account.executable, account.account.owner
                        ),
                    }),
                    Ok(None) => checks.push(DoctorCheck {
                        name: "guard_account",
                        ok: false,
                        detail: "program account not found".into(),
                    }),
                    Err(error) => checks.push(DoctorCheck {
                        name: "guard_account",
                        ok: false,
                        detail: error.to_string(),
                    }),
                }
            }
        }
    }

    if config.jito.enabled {
        if no_network {
            checks.push(DoctorCheck {
                name: "jito",
                ok: true,
                detail: "skipped (--no-network)".into(),
            });
        } else {
            let jito = beast_rpc::JitoClient::new(config.jito.block_engine_urls.clone(), None)?;
            match jito.tip_accounts().await {
                Ok(accounts) => checks.push(DoctorCheck {
                    name: "jito",
                    ok: !accounts.is_empty(),
                    detail: format!("{} tip accounts", accounts.len()),
                }),
                Err(error) => checks.push(DoctorCheck {
                    name: "jito",
                    ok: false,
                    detail: error.to_string(),
                }),
            }
        }
    } else {
        checks.push(DoctorCheck {
            name: "jito",
            ok: true,
            detail: "disabled (radar/canary simulation still uses Jito simulateBundle)".into(),
        });
    }

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": checks.iter().all(|check| check.ok),
                "checks": checks,
            }))?
        );
    } else {
        for check in &checks {
            println!(
                "{} {:<14} {}",
                if check.ok { "OK  " } else { "FAIL" },
                check.name,
                check.detail
            );
        }
        if checks.iter().any(|check| !check.ok) {
            println!();
            println!("Some checks failed. Radar can still run without a wallet or Jito.");
        }
    }
    let _ = wallet_address;
    Ok(())
}

/// Verify a free-tier RPC provider's real limits before wiring it in.
///
/// Every number here comes from the provider's own published docs; the point
/// is that the preset writes limits you cannot accidentally exceed, which is
/// what keeps a free account alive for months instead of hours.
struct ProviderPreset {
    id: &'static str,
    url: &'static str,
    /// URL when `key_env` is empty (public endpoints).
    public_url: Option<&'static str>,
    max_rps: u32,
    heavy_rps: u32,
    monthly_budget: u64,
    /// Honest note shown to the operator when the preset is added.
    caveat: &'static str,
}

// dRPC is deliberately absent: `https://solana.drpc.org` answers a free key
// with JSON-RPC error 35, "chain is not available on free plan, please upgrade
// to paid plan". A paid dRPC account can be added by hand; shipping it as a
// free preset would send operators into a wall the preset promised was open.
const PROVIDER_PRESETS: &[ProviderPreset] = &[
    ProviderPreset {
        id: "helius",
        url: "https://mainnet.helius-rpc.com/?api-key=${KEY}",
        public_url: None,
        max_rps: 8,
        heavy_rps: 4,
        monthly_budget: 800_000,
        caveat:
            "1M credits/month at 10 RPS; getProgramAccounts is 5 RPS and sendTransaction 1 RPS.",
    },
    ProviderPreset {
        id: "alchemy",
        url: "https://solana-mainnet.g.alchemy.com/v2/${KEY}",
        public_url: None,
        max_rps: 12,
        heavy_rps: 4,
        monthly_budget: 900_000,
        caveat: "30M CU/month is ~1.1M requests at ~27 CU average; archival reads on free.",
    },
    ProviderPreset {
        id: "quicknode",
        url: "https://YOUR-ENDPOINT.solana-mainnet.quiknode.pro/${KEY}/",
        public_url: None,
        max_rps: 12,
        heavy_rps: 4,
        monthly_budget: 300_000,
        caveat: "30-day trial of 10M credits at 15 RPS; no permanent free plan.",
    },
    ProviderPreset {
        id: "chainstack",
        url: "https://solana-mainnet.core.chainstack.com/${KEY}/",
        public_url: None,
        max_rps: 12,
        heavy_rps: 4,
        monthly_budget: 2_500_000,
        caveat: "3M RU/month free, but getProgramAccounts is BLOCKED on the Developer plan.",
    },
    ProviderPreset {
        id: "public",
        url: "https://api.mainnet-beta.solana.com",
        public_url: Some("https://api.mainnet-beta.solana.com"),
        max_rps: 4,
        heavy_rps: 1,
        monthly_budget: 0,
        caveat: "100 requests/10s per IP and 40 per method: 4 RPS per method. No SLA.",
    },
];

/// `beast rpc add` — append a verified endpoint block to config.toml.
pub fn rpc_add(
    config_path: Option<&Path>,
    provider: &str,
    key_env: Option<&str>,
    role: &str,
    name: Option<&str>,
    force: bool,
) -> Result<()> {
    let role = match role.to_ascii_lowercase().as_str() {
        "feed" => EndpointRole::Feed,
        "exec" => EndpointRole::Exec,
        "discovery" => EndpointRole::Discovery,
        "rent" => EndpointRole::Rent,
        "fallback" => EndpointRole::Fallback,
        other => bail!("unknown role {other}; use feed|exec|discovery|rent|fallback"),
    };
    let preset = PROVIDER_PRESETS
        .iter()
        .find(|preset| preset.id == provider)
        .ok_or_else(|| {
            let known: Vec<&str> = PROVIDER_PRESETS.iter().map(|p| p.id).collect();
            anyhow::anyhow!("unknown provider {provider}; known: {}", known.join(", "))
        })?;

    let key_env =
        key_env
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(if preset.id == "public" {
                None
            } else {
                Some(preset.id)
            });
    let needs_key = preset.public_url.is_none();
    if needs_key && key_env.is_none() {
        bail!(
            "provider {} needs --key-env (e.g. --key-env HELIUS_API_KEY)",
            preset.id
        );
    }

    let url = if needs_key {
        let key_env = key_env.expect("checked above");
        preset.url.replace("${KEY}", &format!("${{{key_env}}}"))
    } else {
        preset.public_url.expect("public url").to_string()
    };
    let name = name
        .map(str::to_string)
        .unwrap_or_else(|| preset.id.to_string());

    let path = config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("./config.toml"));
    let existed = path.exists();
    let mut text = if existed {
        fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?
    } else {
        format!("config_version = {}\n", beast_core::CONFIG_VERSION)
    };
    if text.contains(&format!("name = \"{name}\"")) && !force {
        bail!("config already has an endpoint named {name}; use --force to add another",);
    }

    let block = format!(
        "\n[[rpc.endpoints]]\nname = \"{name}\"\nurl = \"{url}\"\nrole = \"{role}\"\n         max_rps = {max_rps}\nheavy_rps = {heavy_rps}\nmonthly_call_budget = {budget}\n",
        role = role.as_str(),
        max_rps = preset.max_rps,
        heavy_rps = preset.heavy_rps,
        budget = preset.monthly_budget,
    );
    text.push_str(&block);
    fs::write(&path, &text).with_context(|| format!("write {}", path.display()))?;

    println!("added endpoint {name} ({})", role.as_str());
    println!("  url: {url}");
    if let Some(key_env) = key_env {
        println!("  key: export {key_env}=... (read from the environment, never written here)");
    }
    println!(
        "  limits: {} rps / {} heavy rps / {} calls per month",
        preset.max_rps, preset.heavy_rps, preset.monthly_budget
    );
    println!("  note: {}", preset.caveat);
    if !existed {
        println!("\nwrote {}", path.display());
    }
    println!("\nnext: beast check && beast doctor");
    Ok(())
}

/// `beast rpc list` — every configured endpoint with its role and limits.
pub fn rpc_list(config_path: Option<&Path>, json_output: bool) -> Result<()> {
    let loaded = load(config_path, false)?;
    let endpoints = loaded.config.rpc.effective_endpoints();
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "count": endpoints.len(),
                "endpoints": endpoints,
            }))?
        );
        return Ok(());
    }
    if endpoints.is_empty() {
        println!("no endpoints configured");
        return Ok(());
    }
    println!(
        "{:<12} {:<9} {:>7} {:>7} {:>10}  URL",
        "NAME", "ROLE", "MAX_RPS", "HEAVY", "BUDGET"
    );
    for endpoint in &endpoints {
        println!(
            "{:<12} {:<9} {:>7} {:>7} {:>10}  {}",
            endpoint.name,
            endpoint.role.as_str(),
            endpoint.max_rps,
            endpoint.heavy_rps,
            endpoint.monthly_call_budget,
            endpoint.url,
        );
    }
    Ok(())
}

pub fn init(force: bool) -> Result<()> {
    let path = Path::new("config.toml");
    if path.exists() && !force {
        bail!("config.toml already exists; use --force to overwrite");
    }
    std::fs::write(path, DEFAULT_CONFIG_TOML).context("write config.toml")?;
    println!("wrote {}", path.display());
    println!("next: beast check");
    println!("then: beast run");
    println!("live: set [guard] mode=program + program_id, [jito] enabled=true, and pass --real");
    Ok(())
}

fn load_wallet(config: &BeastConfig, secrets: &config::Secrets) -> Result<Option<Wallet>> {
    if let Some(path) = config
        .wallet
        .keypair_path
        .as_ref()
        .filter(|path| path.exists())
    {
        return Ok(Some(Wallet::load(path, config.wallet.require_0600)?));
    }
    if let Some(json) = secrets.wallet_keypair_json.as_deref() {
        return Ok(Some(Wallet::from_json(json)?));
    }
    Ok(None)
}

pub async fn rent(path: Option<&Path>, real: bool, confirm: bool, json_output: bool) -> Result<()> {
    let loaded = load(path, real)?;
    let config = &loaded.config;
    if !config.rent.enabled {
        bail!("[rent] enabled=false in config");
    }
    let wallet = load_wallet(config, &loaded.secrets)?
        .ok_or_else(|| anyhow::anyhow!("wallet keypair required for rent scan/reclaim"))?;
    let rpc = rpc(config)?;
    let reclaimer = RentReclaimer::new(
        std::sync::Arc::new(rpc),
        std::sync::Arc::new(wallet),
        config.rent.clone(),
    );
    let send = real && confirm;
    if confirm && !real {
        bail!("--confirm requires --real (and runtime.real=true)");
    }
    let report = reclaimer.reclaim(send).await?;
    if json_output {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "rent: scanned={} closable={} attempted={} closed={}",
            report.scanned, report.closable, report.attempted, report.closed
        );
        println!(
            "gross={} fees={} net={} lamports{}",
            report.gross_recovered_lamports,
            report.fee_lamports,
            report.net_recovered_lamports,
            if report.dry_run { " (dry-run)" } else { "" }
        );
        for batch in &report.batches {
            println!(
                "  batch accounts={} recovered={} signature={:?} error={:?}",
                batch.accounts.len(),
                batch.recovered_lamports,
                batch.signature,
                batch.error
            );
        }
        for error in &report.errors {
            println!("  error: {error}");
        }
        if report.dry_run {
            println!("dry-run only; add --real --confirm to send rent-close transactions");
        }
    }
    Ok(())
}

pub fn report(
    path: Option<&Path>,
    out: Option<&Path>,
    csv: Option<&Path>,
    json_output: bool,
) -> Result<()> {
    let loaded = load(path, false)?;
    let store = Store::open(&loaded.config.storage.path)?;
    let strategies = store.strategy_summary()?;
    let settlements = store.list_settlements(100)?;
    let stats = store.stats()?;
    if let Some(csv_path) = csv {
        let mut content = String::from(
            "strategy,opportunities,profitable,modeled_net_lamports,best_net_lamports\n",
        );
        for row in &strategies {
            content.push_str(&format!(
                "{},{},{},{},{}\n",
                row["strategy"].as_str().unwrap_or(""),
                row["opportunities"].as_i64().unwrap_or(0),
                row["profitable"].as_i64().unwrap_or(0),
                row["modeled_net_lamports"].as_i64().unwrap_or(0),
                row["best_net_lamports"].as_i64().unwrap_or(0),
            ));
        }
        std::fs::write(csv_path, content)
            .with_context(|| format!("write {}", csv_path.display()))?;
    }
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "stats": stats,
                "strategies": strategies,
                "settlements": settlements,
            }))?
        );
        return Ok(());
    }
    let total_settled: i64 = settlements.iter().map(|s| s.realized_net_lamports).sum();
    let model_error: f64 = if settlements.is_empty() {
        0.0
    } else {
        settlements
            .iter()
            .map(|s| s.model_error_bps() as f64)
            .sum::<f64>()
            / settlements.len() as f64
    };
    let mut markdown = String::new();
    markdown.push_str("# SOL BEAST radar report\n\n");
    markdown.push_str(&format!(
        "- Opportunities: {}\n- Attempts: {}\n- Landed: {}\n- Reverted: {}\n- Bundles: {}\n- Settlements: {}\n- Realized net: {} lamports\n- Mean model error: {:.1} bps\n\n",
        stats.opportunities,
        stats.attempts,
        stats.landed,
        stats.reverted,
        stats.bundles,
        stats.settlements,
        if stats.realized_net_lamports != 0 {
            stats.realized_net_lamports
        } else {
            total_settled
        },
        model_error,
    ));
    markdown.push_str("## Strategy summary\n\n");
    markdown.push_str("| Strategy | Opportunities | Profitable | Modeled net | Best net |\n|---|---:|---:|---:|---:|\n");
    for row in &strategies {
        markdown.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            row["strategy"].as_str().unwrap_or(""),
            row["opportunities"].as_i64().unwrap_or(0),
            row["profitable"].as_i64().unwrap_or(0),
            row["modeled_net_lamports"].as_i64().unwrap_or(0),
            row["best_net_lamports"].as_i64().unwrap_or(0),
        ));
    }
    if let Some(out) = out {
        std::fs::write(out, &markdown).with_context(|| format!("write {}", out.display()))?;
        println!("wrote {}", out.display());
    } else {
        print!("{markdown}");
    }
    Ok(())
}
