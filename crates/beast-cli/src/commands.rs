//! Non-daemon CLI commands.

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use beast_core::config::{
    self, BeastConfig, EndpointRole, GuardMode, LoadedConfig, RpcEndpointConfig,
    DEFAULT_CONFIG_TOML,
};
use beast_core::types::{LegPlan, Mode, Opportunity, OutcomeLabel, StrategyKind};
use beast_dex::catalog::{self, VenueKind, VENUES};
use beast_dex::decode::{pool_layout, PoolLayout};
pub(crate) use beast_dex::jupiter::JupiterClient;
use beast_dex::jupiter::QuoteRequest;
use beast_dex::Address;
use beast_exec::{cpmm, RentReclaimer, Wallet, WSOL_MINT};
use beast_feed::{refresh_pool, WatchedPool};
use beast_rpc::{EndpointOptions, JitoClient, RpcClient};
use beast_store::Store;
use serde::Serialize;
use serde_json::{json, Value};
#[allow(deprecated)]
use solana_sdk::address_lookup_table::instruction as alt_instruction;
#[allow(deprecated)]
use solana_sdk::address_lookup_table::state::AddressLookupTable as LookupTableState;
use solana_sdk::clock::Slot;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::transaction::Transaction;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

use crate::engine::{build_executor, build_flash_runtime};

fn load(path: Option<&Path>, real: bool) -> Result<LoadedConfig> {
    let loaded = config::load(path, real)?;
    for warning in &loaded.warnings {
        eprintln!("warning: {warning}");
    }
    Ok(loaded)
}

pub(crate) fn rpc(config: &BeastConfig) -> Result<RpcClient> {
    rpc_with_timeout(config, config.rpc.timeout_ms)
}

/// Build a client using the same role-routed endpoints as the engine.
///
/// The old helper passed only `rpc.urls`, so `beast discover` silently used the
/// public feed endpoint instead of the configured Helius discovery endpoint and
/// timed out on a full program scan.
fn rpc_with_timeout(config: &BeastConfig, timeout_ms: u64) -> Result<RpcClient> {
    let endpoints = config
        .rpc
        .effective_endpoints()
        .into_iter()
        .map(|endpoint| {
            EndpointOptions::new(endpoint.name, endpoint.url)
                .with_role_name(endpoint.role.as_str())
                .with_rps(endpoint.max_rps, endpoint.heavy_rps)
                .with_budget(endpoint.monthly_call_budget)
                .with_priority(endpoint.priority)
        })
        .collect();
    Ok(RpcClient::with_endpoints(
        endpoints,
        config.rpc.commitment.clone(),
        timeout_ms,
        config.rpc.max_retries,
        config.rpc.circuit_breaker_seconds * 1_000,
    )?)
}

pub(crate) fn jupiter(config: &BeastConfig, secrets: &config::Secrets) -> Result<JupiterClient> {
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

/// Build the `getProgramAccounts` filters for mint-aware discovery.
///
/// Two mints means "pools containing this pair": both account orientations are
/// queried, because a base/quote pair can be stored in either order. One mint
/// means "pools containing this mint on either side". No mint keeps the old
/// full-program scan, which is only practical for small programs or a paid RPC.
fn discovery_filters(layout: Option<PoolLayout>, mints: &[Address]) -> Result<Vec<Vec<Value>>> {
    let Some(layout) = layout else {
        if mints.is_empty() {
            return Ok(vec![Vec::new()]);
        }
        bail!("venue has no mint offsets, so --mint cannot be used");
    };
    let mut base = Vec::new();
    if let Some(size) = layout.account_size {
        base.push(json!({ "dataSize": size }));
    }
    let mint_filter = |offset: usize, mint: &Address| json!({ "memcmp": { "offset": offset, "bytes": mint.to_string() } });
    let filters = match mints {
        [] => vec![base],
        [mint] => vec![
            {
                let mut filters = base.clone();
                filters.push(mint_filter(layout.mint_a_offset, mint));
                filters
            },
            {
                let mut filters = base.clone();
                filters.push(mint_filter(layout.mint_b_offset, mint));
                filters
            },
        ],
        [a, b] => vec![
            {
                let mut filters = base.clone();
                filters.push(mint_filter(layout.mint_a_offset, a));
                filters.push(mint_filter(layout.mint_b_offset, b));
                filters
            },
            {
                let mut filters = base.clone();
                filters.push(mint_filter(layout.mint_a_offset, b));
                filters.push(mint_filter(layout.mint_b_offset, a));
                filters
            },
        ],
        _ => bail!("--mint may be given at most twice (a pair of mints)"),
    };
    Ok(filters)
}

pub async fn discover(
    path: Option<&Path>,
    venue_id: &str,
    mints: &[String],
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
    let mints = mints
        .iter()
        .map(|mint| {
            mint.parse::<Address>()
                .map_err(|error| anyhow::anyhow!("--mint {mint}: {error}"))
        })
        .collect::<Result<Vec<_>>>()?;
    if mints.len() == 2 && mints[0] == mints[1] {
        bail!("--mint must name two different mints");
    }
    let layout = venue.decoder.and_then(pool_layout);
    let filters = discovery_filters(layout, &mints)?;
    if mints.is_empty() {
        eprintln!(
            "warning: no --mint filter; scanning every {} account can exceed free-tier limits",
            venue.id
        );
    }
    // Discovery is a heavy call and a filtered scan can still take longer than
    // the five-second feed timeout. Give it its own budget instead of failing
    // halfway through a valid request.
    let rpc = rpc_with_timeout(config, config.rpc.timeout_ms.max(60_000))?;
    let program_id = venue
        .program_id
        .parse::<Address>()
        .map_err(|error| anyhow::anyhow!("venue program id: {error}"))?;
    eprintln!(
        "discovering {} ({}){}{} ...",
        venue.id,
        venue.program_id,
        if mints.is_empty() {
            String::new()
        } else {
            format!(
                " for {}",
                mints
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" / ")
            )
        },
        if mints.len() == 2 { " [pair]" } else { "" }
    );
    let mut by_address: std::collections::BTreeMap<Address, ()> = std::collections::BTreeMap::new();
    for filter in filters {
        let accounts = rpc
            .get_program_accounts(program_id, filter, Some((0, 0)))
            .await
            .with_context(|| {
                format!(
                    "getProgramAccounts {} (pass --mint <A> --mint <B> to narrow the scan)",
                    venue.program_id
                )
            })?;
        by_address.extend(accounts.into_iter().map(|(address, _)| (address, ())));
    }
    let accounts: Vec<Address> = by_address.into_keys().collect();
    eprintln!(
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
    for address in accounts.into_iter().take(max_attempts) {
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
                "mints": mints,
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

/// Quote one CPMM leg from live vault balances, for display and plan metadata.
async fn cpmm_leg_quote(
    rpc: &RpcClient,
    pool: &cpmm::CpmmPool,
    input_mint: &Pubkey,
    output_mint: &Pubkey,
    amount: u64,
) -> Result<(u64, u32)> {
    let input_index = pool.mint_index(input_mint).ok_or_else(|| {
        anyhow::anyhow!(
            "pool {} does not contain input mint {input_mint}",
            pool.address
        )
    })?;
    let output_index = 1 - input_index;
    if pool.mints[output_index] != *output_mint {
        bail!(
            "pool {} does not contain output mint {output_mint}",
            pool.address
        );
    }
    let keys = [
        beast_flash::address(&pool.amm_config),
        beast_flash::address(&pool.vaults[input_index]),
        beast_flash::address(&pool.vaults[output_index]),
    ];
    let accounts = rpc.get_multiple_accounts(&keys).await?;
    let config = accounts
        .first()
        .and_then(Option::as_ref)
        .ok_or_else(|| anyhow::anyhow!("CPMM config {} not found", pool.amm_config))?;
    let vault_in = accounts
        .get(1)
        .and_then(Option::as_ref)
        .ok_or_else(|| anyhow::anyhow!("CPMM vault {} not found", pool.vaults[input_index]))?;
    let vault_out = accounts
        .get(2)
        .and_then(Option::as_ref)
        .ok_or_else(|| anyhow::anyhow!("CPMM vault {} not found", pool.vaults[output_index]))?;
    let fee = cpmm::decode_trade_fee_rate(&config.data)?;
    let (mint_in, reserve_in) = cpmm::token_account_mint_and_amount(&vault_in.data)?;
    let (mint_out, reserve_out) = cpmm::token_account_mint_and_amount(&vault_out.data)?;
    if mint_in != *input_mint || mint_out != *output_mint {
        bail!(
            "CPMM vault mints do not match the pool layout for {}",
            pool.address
        );
    }
    // The swap curve runs on vault balances minus booked-but-uncollected
    // protocol/fund/creator fees, exactly like the on-chain program.
    let reserve_in = pool.effective_reserve(input_index, reserve_in)?;
    let reserve_out = pool.effective_reserve(output_index, reserve_out)?;
    let amount_out = cpmm::quote_base_input(amount, reserve_in, reserve_out, fee)?;
    let spot = (u128::from(amount))
        .checked_mul(u128::from(reserve_out))
        .and_then(|value| value.checked_div(u128::from(reserve_in)))
        .unwrap_or(0);
    let impact = if spot > u128::from(amount_out) {
        u32::try_from((spot - u128::from(amount_out)) * 10_000 / spot).unwrap_or(10_000)
    } else {
        0
    };
    Ok((amount_out, impact))
}

/// `beast flash-alt` — create an address lookup table for the configured
/// flash provider's stable accounts.
///
/// Marginfi wraps borrow/repay in four extra instructions on a temporary
/// account. With two Raydium CPMM swaps the static-key message exceeds the
/// 1232-byte limit by about 94 bytes. A table carrying only the provider's
/// long-lived accounts removes ~30 bytes per account and keeps per-cycle
/// accounts static.
///
/// The table address is derived from the wallet and the creation slot, so no
/// extra keypair is needed. Simulation is the default; sending requires
/// `--real --confirm` **and** `runtime.real = true`.
pub async fn flash_alt(
    path: Option<&Path>,
    real: bool,
    confirm: bool,
    json_output: bool,
) -> Result<()> {
    if confirm && !real {
        bail!("--confirm requires --real");
    }
    let loaded = load(path, real)?;
    let config = &loaded.config;
    if !config.flash.enabled {
        bail!("[flash] enabled=false in config");
    }
    if real && !config.runtime.real {
        bail!("--real requires runtime.real=true in config");
    }
    let wallet = load_wallet(config, &loaded.secrets)?
        .ok_or_else(|| anyhow::anyhow!("wallet keypair required to create a lookup table"))?;
    let rpc = Arc::new(rpc(config)?);
    let flash = build_flash_runtime(config, &rpc)
        .await?
        .ok_or_else(|| anyhow::anyhow!("[flash] enabled=false in config"))?;
    let wsol: Address = WSOL_MINT.parse().expect("WSOL is a valid address");
    let reserve = flash
        .reserve_for(wsol)
        .or_else(|| {
            flash
                .reserves
                .values()
                .max_by_key(|reserve| reserve.available)
        })
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no active flash reserve was loaded"))?;

    // Only long-lived data accounts belong in the table. Signers must stay
    // static; program ids stay static too, so a table cannot change how a
    // program is resolved.
    let provider = flash.provider.clone();
    let forbidden = [
        wallet.address(),
        beast_flash::address(&provider.program_id()),
        beast_flash::address(&solana_sdk::system_program::ID),
        beast_flash::address(&solana_sdk::compute_budget::ID),
    ];
    let mut addresses: Vec<Address> = Vec::new();
    let mut push = |address: Address| {
        if address != Address::ZERO
            && !forbidden.contains(&address)
            && !addresses.contains(&address)
        {
            addresses.push(address);
        }
    };
    push(reserve.reserve);
    push(reserve.supply_vault);
    push(reserve.fee_vault);
    push(reserve.lending_market);
    push(reserve.lending_market_authority);
    for address in &reserve.remaining_accounts {
        push(*address);
    }
    if provider.id() == "marginfi" {
        push(Address::from(
            *solana_sdk::sysvar::instructions::ID.as_array(),
        ));
    }
    if addresses.is_empty() {
        bail!("reserve {} has no table-eligible accounts", reserve.reserve);
    }
    if addresses.len() > 20 {
        bail!(
            "reserve {} needs {} table accounts; the extend instruction accepts at most 20",
            reserve.reserve,
            addresses.len()
        );
    }

    let slot = rpc.get_slot().await?;
    let (create, table) =
        alt_instruction::create_lookup_table(wallet.pubkey(), wallet.pubkey(), slot);
    let extend = alt_instruction::extend_lookup_table(
        table,
        wallet.pubkey(),
        Some(wallet.pubkey()),
        addresses.iter().map(beast_flash::pubkey).collect(),
    );
    let (blockhash, _) = rpc.get_latest_blockhash().await?;
    let blockhash: solana_sdk::hash::Hash = blockhash
        .parse()
        .map_err(|error| anyhow::anyhow!("latest blockhash: {error}"))?;
    let transaction = Transaction::new_signed_with_payer(
        &[create, extend],
        Some(&wallet.pubkey()),
        &[wallet.keypair()],
        blockhash,
    );
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(bincode::serialize(&transaction).context("serialize lookup-table setup")?);
    let size = encoded.len();
    // Rent is a cluster parameter and changed on mainnet in 2026; query it
    // instead of assuming `Rent::default()`.
    let rent = rpc
        .get_minimum_balance_for_rent_exemption(56 + 32 * addresses.len())
        .await?;
    let fee = 5_000u64; // one signature at the base rate
    let wallet_balance = rpc.get_balance(wallet.address()).await?;
    let total = rent.saturating_add(fee);
    let remaining = wallet_balance.saturating_sub(total);
    let simulation = rpc.simulate_transaction(&encoded).await?;
    let simulation_error = simulation.err.clone();
    let mut signature: Option<String> = None;
    let mut sent = false;
    if simulation_error.is_none() && real && confirm {
        signature = Some(rpc.send_transaction(&encoded, false).await?);
        sent = true;
    }

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "table": table.to_string(),
                "slot": slot,
                "provider": provider.id(),
                "reserve": reserve.reserve.to_string(),
                "addresses": addresses.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "rent_lamports": rent,
                "fee_lamports": fee,
                "total_cost_lamports": total,
                "wallet_balance": wallet_balance,
                "wallet_after": remaining,
                "transaction_bytes": size,
                "simulation": simulation,
                "sent": sent,
                "signature": signature,
                "config_line": format!("lookup_table = \"{table}\""),
            }))?
        );
    } else {
        println!("provider : {}", provider.id());
        println!("reserve  : {}", reserve.reserve);
        println!("table    : {table} (derived from wallet + slot {slot})");
        println!("accounts : {}", addresses.len());
        for address in &addresses {
            println!("  {address}");
        }
        println!(
            "cost     : {rent} lamports rent + {fee} lamports fee = {total} \
             (wallet {wallet_balance} -> {remaining})"
        );
        println!("tx bytes : {size} (simulated)");
        if let Some(error) = &simulation_error {
            println!("simulation FAILED: {error}");
        } else {
            println!("simulation: ok");
        }
        if let Some(signature) = &signature {
            println!("sent: {signature}");
            println!(
                "wait one slot, then set `lookup_table = \"{table}\"` and run the flash probe"
            );
        } else if simulation_error.is_none() {
            println!("\nAdd this line to [flash] after the table is active:");
            println!("lookup_table = \"{table}\"");
            if !real || !confirm {
                println!("\ndry-run only; add --real --confirm to send the setup transaction");
            }
        }
    }
    if let Some(error) = simulation_error {
        return Err(anyhow::anyhow!(
            "lookup-table setup simulation failed: {error}"
        ));
    }
    Ok(())
}

/// `beast flash-alt-close` — recover the rent of a flash lookup table.
///
/// A table must be deactivated first and then wait out the slot-hash cool-down
/// (about 512 slots) before it can be closed. The command is idempotent: run it
/// once to deactivate, then again after the cool-down to close and return the
/// rent to the wallet. Simulation is the default; sending requires
/// `--real --confirm` **and** `runtime.real = true`.
pub async fn flash_alt_close(
    path: Option<&Path>,
    table: &str,
    real: bool,
    confirm: bool,
    json_output: bool,
) -> Result<()> {
    if confirm && !real {
        bail!("--confirm requires --real");
    }
    let loaded = load(path, real)?;
    let config = &loaded.config;
    if real && !config.runtime.real {
        bail!("--real requires runtime.real=true in config");
    }
    let wallet = load_wallet(config, &loaded.secrets)?
        .ok_or_else(|| anyhow::anyhow!("wallet keypair required to manage a lookup table"))?;
    let table_key = Pubkey::from_str(table).with_context(|| format!("lookup table {table:?}"))?;
    let rpc = Arc::new(rpc(config)?);
    let account = rpc
        .get_account_info(beast_flash::address(&table_key))
        .await?
        .ok_or_else(|| anyhow::anyhow!("lookup table {table_key} not found on chain"))?;
    let program = solana_sdk::address_lookup_table::program::id();
    if account.account.owner != beast_flash::address(&program) {
        bail!(
            "account {table_key} is owned by {}, not the address lookup table program",
            account.account.owner
        );
    }
    let state = LookupTableState::deserialize(&account.account.data)
        .map_err(|error| anyhow::anyhow!("decode lookup table {table_key}: {error:?}"))?;
    let authority = state
        .meta
        .authority
        .ok_or_else(|| anyhow::anyhow!("lookup table {table_key} has no authority"))?;
    if authority != wallet.pubkey() {
        bail!(
            "lookup table {table_key} is controlled by {authority}, not the configured wallet {}",
            wallet.pubkey()
        );
    }
    let deactivation_slot = state.meta.deactivation_slot;
    let active = deactivation_slot == Slot::MAX;
    let instruction = if active {
        alt_instruction::deactivate_lookup_table(table_key, wallet.pubkey())
    } else {
        alt_instruction::close_lookup_table(table_key, wallet.pubkey(), wallet.pubkey())
    };
    let (blockhash, _) = rpc.get_latest_blockhash().await?;
    let blockhash: solana_sdk::hash::Hash = blockhash
        .parse()
        .map_err(|error| anyhow::anyhow!("latest blockhash: {error}"))?;
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&wallet.pubkey()),
        &[wallet.keypair()],
        blockhash,
    );
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(bincode::serialize(&transaction).context("serialize lookup-table transaction")?);
    let simulation = rpc.simulate_transaction(&encoded).await?;
    let simulation_error = simulation.err.clone();
    let mut signature: Option<String> = None;
    if simulation_error.is_none() && real && confirm {
        signature = Some(rpc.send_transaction(&encoded, false).await?);
    }

    let action = if active { "deactivate" } else { "close" };
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "table": table_key.to_string(),
                "action": action,
                "authority": authority.to_string(),
                "deactivation_slot": deactivation_slot,
                "rent_lamports": if active { 0 } else { account.account.lamports },
                "simulation": simulation,
                "sent": signature.is_some(),
                "signature": signature,
                "next_step": if active {
                    "run the same command again after about 512 slots to close the table and recover its rent"
                } else {
                    "rent returned to the wallet"
                },
            }))?
        );
    } else if active {
        println!("table     : {table_key}");
        println!("authority : {authority}");
        println!("action    : deactivate (rent stays locked until close)");
        if let Some(error) = &simulation_error {
            println!("simulation FAILED: {error}");
        } else {
            println!("simulation: ok");
        }
        if let Some(signature) = &signature {
            println!("sent: {signature}");
            println!("run the same command again after about 512 slots to close it");
        } else if simulation_error.is_none() {
            println!("\ndry-run only; add --real --confirm to deactivate");
        }
    } else {
        println!("table     : {table_key}");
        println!("authority : {authority}");
        println!("deactivated at slot {deactivation_slot}");
        println!(
            "action    : close and return {} lamports",
            account.account.lamports
        );
        if let Some(error) = &simulation_error {
            println!("simulation FAILED: {error}");
            println!("the cool-down has not elapsed; wait and run this command again");
        } else {
            println!("simulation: ok");
        }
        if let Some(signature) = &signature {
            println!("sent: {signature}");
        } else if simulation_error.is_none() {
            println!("\ndry-run only; add --real --confirm to close");
        }
    }
    if let Some(error) = simulation_error {
        return Err(anyhow::anyhow!(
            "lookup-table {action} simulation failed: {error}"
        ));
    }
    Ok(())
}

/// `beast flash-cycle` — prove a two-pool Raydium CPMM flash cycle.
///
/// Simulation only: this command never signs-and-sends a bundle. It builds the
/// exact borrow/swap/repay transaction and simulates it (through Jito when
/// `--jito` is passed, otherwise through the configured RPC).
pub async fn flash_cycle(
    path: Option<&Path>,
    pool_in: &str,
    pool_out: &str,
    amount: u64,
    jito_bundle: bool,
    json_output: bool,
    probe: bool,
) -> Result<()> {
    if amount == 0 {
        bail!("--amount must be > 0");
    }
    let loaded = load(path, false)?;
    let config = &loaded.config;
    if !config.flash.enabled {
        bail!("[flash] enabled=false in config");
    }
    if amount > config.flash.max_borrow_lamports {
        bail!(
            "--amount {amount} exceeds flash.max_borrow_lamports {}",
            config.flash.max_borrow_lamports
        );
    }
    let wallet = load_wallet(config, &loaded.secrets)?
        .ok_or_else(|| anyhow::anyhow!("wallet keypair required for a flash-cycle simulation"))?;
    let rpc = Arc::new(rpc(config)?);
    let flash = build_flash_runtime(config, &rpc)
        .await?
        .ok_or_else(|| anyhow::anyhow!("[flash] enabled=false in config"))?;
    let wsol: Address = WSOL_MINT.parse().expect("WSOL is a valid address");
    let reserve = flash
        .reserve_for(wsol)
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no active WSOL flash reserve was loaded; clear flash.mint/flash.reserve or point them at WSOL"
            )
        })?;

    let pool_a_address = pool_in
        .parse::<Address>()
        .map_err(|error| anyhow::anyhow!("pool-in: {error}"))?;
    let pool_b_address = pool_out
        .parse::<Address>()
        .map_err(|error| anyhow::anyhow!("pool-out: {error}"))?;
    if pool_a_address == pool_b_address {
        bail!("pool-in and pool-out must be different pools");
    }
    let accounts = rpc
        .get_multiple_accounts(&[pool_a_address, pool_b_address])
        .await?;
    let decode_pool = |address: Address,
                       account: Option<beast_rpc::Account>|
     -> Result<cpmm::CpmmPool> {
        let account = account.ok_or_else(|| anyhow::anyhow!("CPMM pool {address} not found"))?;
        Ok(cpmm::CpmmPool::decode(
            beast_flash::pubkey(&address),
            &account.data,
        )?)
    };
    let pool_a = decode_pool(pool_a_address, accounts.first().cloned().flatten())?;
    let pool_b = decode_pool(pool_b_address, accounts.get(1).cloned().flatten())?;
    pool_a.ensure_swappable()?;
    pool_b.ensure_swappable()?;
    pool_a.authority()?;
    pool_b.authority()?;

    let wsol_key = Pubkey::from_str(WSOL_MINT).expect("WSOL is a valid pubkey");
    let a_in = pool_a
        .mint_index(&wsol_key)
        .ok_or_else(|| anyhow::anyhow!("pool-in {} does not contain WSOL", pool_a.address))?;
    let middle = pool_a.mints[1 - a_in];
    let b_in = pool_b.mint_index(&middle).ok_or_else(|| {
        anyhow::anyhow!(
            "pool-out {} does not contain the intermediate mint {middle}",
            pool_b.address
        )
    })?;
    if pool_b.mints[1 - b_in] != wsol_key {
        bail!(
            "pool-out {} does not return WSOL; pool-in/pool-out are not a WSOL cycle",
            pool_b.address
        );
    }

    let (quote_a, impact_a) = cpmm_leg_quote(&rpc, &pool_a, &wsol_key, &middle, amount).await?;
    let (quote_b, impact_b) = cpmm_leg_quote(&rpc, &pool_b, &middle, &wsol_key, quote_a).await?;
    let provider = flash.provider.clone();
    let flash_fee = provider.flash_fee(&reserve, amount)?;
    let gross = i64::try_from(quote_b)
        .ok()
        .and_then(|value| value.checked_sub(i64::try_from(amount).ok()?))
        .ok_or_else(|| anyhow::anyhow!("CPMM quote overflow"))?;

    let slot = rpc.get_slot().await.unwrap_or(0);
    let opportunity = Opportunity {
        id: Uuid::new_v4(),
        strategy: StrategyKind::FlashArb,
        mode: Mode::Canary,
        slot_detected: slot,
        detected_at_ms: chrono::Utc::now().timestamp_millis(),
        legs: vec![
            LegPlan {
                venue: "raydium_cpmm".into(),
                pool: pool_a.address.to_string(),
                mint_in: WSOL_MINT.into(),
                mint_out: middle.to_string(),
                amount_in: amount,
                min_out: quote_a,
                price_impact_bps: impact_a,
            },
            LegPlan {
                venue: "raydium_cpmm".into(),
                pool: pool_b.address.to_string(),
                mint_in: middle.to_string(),
                mint_out: WSOL_MINT.into(),
                amount_in: quote_a,
                min_out: quote_b,
                price_impact_bps: impact_b,
            },
        ],
        est_gross_lamports: gross,
        est_costs: beast_core::types::CostBreakdown {
            flash_fee_lamports: flash_fee,
            ..beast_core::types::CostBreakdown::default()
        },
        est_net_lamports: gross.saturating_sub(flash_fee as i64),
        confidence: 1.0,
        ttl_slots: 1,
        dedupe_key: format!("flash_cycle:{pool_in}:{pool_out}:{amount}"),
        outcome: OutcomeLabel::Open,
        meta: json!({"source": "beast flash-cycle"}),
    };

    let jupiter_client = Arc::new(jupiter(config, &loaded.secrets)?);
    let jito = if config.jito.enabled {
        Some(Arc::new(JitoClient::new(
            config.jito.block_engine_urls.clone(),
            None,
        )?))
    } else {
        None
    };
    let wallet = Arc::new(wallet);
    let executor = build_executor(
        config,
        &rpc,
        &jupiter_client,
        &jito,
        Some(wallet),
        Mode::Canary,
        false,
    )?
    .ok_or_else(|| anyhow::anyhow!("executor unavailable"))?;

    let plan = if probe {
        executor
            .prepare_flash_cycle_probe(&opportunity, provider.as_ref(), &reserve)
            .await?
    } else {
        executor
            .prepare_flash_cycle(&opportunity, provider.as_ref(), &reserve)
            .await?
    };

    let (simulation, used_jito) = if jito_bundle {
        if !executor.jito_enabled() {
            bail!("--jito requested but [jito] is disabled");
        }
        (executor.simulate(&plan).await?, true)
    } else {
        (
            serde_json::to_value(executor.simulate_cycle(&plan).await?)?,
            false,
        )
    };
    let simulation_error = if used_jito {
        None
    } else {
        simulation
            .get("err")
            .cloned()
            .filter(|value| !value.is_null())
    };

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "plan": plan,
                "jito_bundle": used_jito,
                "simulation": simulation,
                "ok": simulation_error.is_none(),
                "probe": probe,
            }))?
        );
    } else {
        let flash_info = plan.flash.as_ref().expect("flash plan carries info");
        if probe {
            println!(
                "MECHANICS PROBE: this cycle is expected to lose money; the wallet prefunds the \
                 shortfall and the transaction is simulated only, never sent."
            );
        }
        println!(
            "plan: borrow {} {} from {} (fee {})",
            flash_info.borrowed, flash_info.mint, flash_info.provider, flash_info.fee
        );
        println!(
            "      repay {} | enforced final min {} | tip {} | costs {}",
            flash_info.repayment, plan.quoted_out_min, plan.tip_lamports, plan.fees_lamports
        );
        println!(
            "      pools {} -> {} | transactions {} | guard_required {}",
            pool_a.address,
            pool_b.address,
            plan.encoded_transactions.len(),
            plan.guard_required_lamports
        );
        println!(
            "simulation: {} ({} )",
            if simulation_error.is_none() {
                "ok"
            } else {
                "FAILED"
            },
            if used_jito {
                "Jito bundle"
            } else {
                "RPC cycle"
            }
        );
        if let Some(error) = &simulation_error {
            println!("error: {error}");
        }
        if let Some(logs) = simulation.get("logs").and_then(Value::as_array) {
            let start = logs.len().saturating_sub(12);
            for log in &logs[start..] {
                if let Some(log) = log.as_str() {
                    println!("  {log}");
                }
            }
        }
    }
    if let Some(error) = simulation_error {
        bail!("flash cycle simulation failed: {error}");
    }
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

pub(crate) fn load_wallet(
    config: &BeastConfig,
    secrets: &config::Secrets,
) -> Result<Option<Wallet>> {
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

/// Simulate one AMM v4 swap against mainnet and report the program's verdict.
///
/// This is the proof step for the AMM v4 builder: if the simulation succeeds,
/// the instruction id, the account list and the quoted minimum are all valid
/// against the live program. Nothing is signed into a block and nothing is
/// spent, so it is safe to run with a tiny wallet.
pub async fn amm_v4_probe(
    path: Option<&Path>,
    pool: &str,
    amount: u64,
    slippage_bps: u16,
    json_output: bool,
) -> Result<()> {
    let loaded = load(path, false)?;
    let config = &loaded.config;
    let wallet = load_wallet(config, &loaded.secrets)?
        .ok_or_else(|| anyhow::anyhow!("wallet keypair required for the AMM v4 probe"))?;
    let rpc = rpc(config)?;
    let pool_address = Address::from_str(pool).context("pool address")?;
    let balance = rpc.get_balance(wallet.address()).await?;
    if balance < amount.saturating_add(3_000_000) {
        bail!(
            "wallet holds {balance} lamports; the probe needs the swap amount plus ATA rent and fees"
        );
    }

    let outcome =
        beast_exec::amm_v4::probe_swap(&rpc, &wallet, pool_address, amount, slippage_bps).await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!("pool        {}", outcome.pool);
        println!(
            "direction   {} -> {}",
            outcome.input_mint, outcome.output_mint
        );
        println!("amount      {}", outcome.amount_in);
        println!(
            "reserves    in={} out={}",
            outcome.reserve_in, outcome.reserve_out
        );
        println!("quoted      {}", outcome.quoted_out);
        if let Some(actual) = outcome.actual_out {
            println!("actual      {actual}");
        }
        if let Some(error) = outcome.quote_error {
            println!("quote error {error}");
        }
        println!("minimum     {}", outcome.minimum_out);
        println!(
            "pnl         coin={} pc={}",
            outcome.need_take_pnl_coin, outcome.need_take_pnl_pc
        );
        println!(
            "result      {}",
            if outcome.ok() {
                "ok".to_string()
            } else {
                format!("{:?}", outcome.err)
            }
        );
        if let Some(units) = outcome.units_consumed {
            println!("compute     {units}");
        }
        for line in &outcome.logs {
            println!("  log {line}");
        }
    }

    if !outcome.ok() {
        bail!("AMM v4 simulation failed");
    }
    Ok(())
}

/// Quote and simulate one Whirlpool swap on mainnet.
pub async fn whirlpool_probe(
    path: Option<&Path>,
    pool: &str,
    amount: u64,
    slippage_bps: u16,
    json_output: bool,
) -> Result<()> {
    let loaded = load(path, false)?;
    let config = &loaded.config;
    let wallet = load_wallet(config, &loaded.secrets)?
        .ok_or_else(|| anyhow::anyhow!("wallet keypair required for the Whirlpool probe"))?;
    let rpc = rpc(config)?;
    let pool_address = Address::from_str(pool).context("pool address")?;
    let balance = rpc.get_balance(wallet.address()).await?;
    if balance < amount.saturating_add(3_000_000) {
        bail!(
            "wallet holds {balance} lamports; the probe needs the swap amount plus ATA rent and fees"
        );
    }

    let outcome =
        beast_exec::whirlpool::probe_swap(&rpc, &wallet, pool_address, amount, slippage_bps).await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&outcome)?);
    } else {
        println!("pool        {}", outcome.pool);
        println!(
            "direction   {} -> {} ({})",
            outcome.input_mint,
            outcome.output_mint,
            if outcome.a_to_b { "a->b" } else { "b->a" }
        );
        println!("amount      {}", outcome.amount_in);
        println!(
            "state       sqrt={} liquidity={} tick={} fee={}",
            outcome.sqrt_price,
            outcome.liquidity,
            outcome.tick_current_index,
            outcome.fee_rate
        );
        println!("arrays      {}", outcome.tick_arrays.join(", "));
        println!("quoted      {}", outcome.quoted_out);
        if let Some(reverse) = outcome.reverse_quoted {
            let sign = if reverse <= outcome.amount_in { "-" } else { "+" };
            let delta = outcome.amount_in.abs_diff(reverse);
            println!("round trip  {reverse} ({sign}{delta} vs input)");
        }
        if let Some(reverse) = outcome.reverse_quoted {
            let sign = if reverse <= outcome.amount_in { "-" } else { "+" };
            let delta = outcome.amount_in.abs_diff(reverse);
            println!("round trip  {reverse} ({sign}{delta} vs input)");
        }
        if let Some(actual) = outcome.actual_out {
            println!("actual      {actual}");
        }
        if let Some(error) = outcome.quote_error {
            println!("quote error {error}");
        }
        println!("minimum     {}", outcome.minimum_out);
        println!(
            "result      {}",
            if outcome.ok() {
                "ok".to_string()
            } else {
                format!("{:?}", outcome.err)
            }
        );
        if let Some(units) = outcome.units_consumed {
            println!("compute     {units}");
        }
        for line in &outcome.logs {
            println!("  log {line}");
        }
    }

    if !outcome.ok() {
        bail!("Whirlpool simulation failed");
    }
    Ok(())
}
