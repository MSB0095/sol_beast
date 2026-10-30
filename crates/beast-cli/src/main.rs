//! `beast` command line.

mod arb;
mod commands;
mod engine;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "beast",
    version,
    about = "SOL BEAST 2.0: green-or-revert Solana keeper engine",
    long_about = "Radar, simulate and guard-gate Solana liquidity opportunities. \
                  Live execution is disabled by default and always requires --real, \
                  runtime.real=true, a verified guard and Jito."
)]
struct Cli {
    /// Path to config.toml. Defaults to ./config.toml, then embedded defaults.
    #[arg(long, global = true, env = "BEAST_CONFIG")]
    config: Option<PathBuf>,

    /// JSON logs.
    #[arg(long, global = true)]
    json_logs: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the radar, feed and local API.
    Run {
        /// Explicit live-execution acknowledgement.
        #[arg(long)]
        real: bool,
        /// Pool refresh interval. Defaults to 1500 ms.
        #[arg(long)]
        feed_interval_ms: Option<u64>,
        /// Radar scan interval. Defaults to 750 ms.
        #[arg(long, default_value_t = 750)]
        radar_interval_ms: u64,
    },
    /// Validate configuration and report readiness without network calls.
    Check {
        #[arg(long)]
        real: bool,
        #[arg(long)]
        json: bool,
    },
    /// List every registered Solana venue and coverage level.
    Venues {
        #[arg(long)]
        json: bool,
        #[arg(long)]
        kind: Option<String>,
    },
    /// Discover pools for one venue via getProgramAccounts.
    Discover {
        #[arg(long)]
        venue: String,
        /// Mint to filter by. Repeat twice to discover one mint pair.
        #[arg(long = "mint")]
        mint: Vec<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Persist decoded pools to the configured SQLite store.
        #[arg(long)]
        save: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build and simulate a two-pool Raydium CPMM flash cycle.
    ///
    /// Simulation only: this command never sends a bundle. It is the cheapest
    /// way to prove borrow -> swap -> swap -> repay with very little SOL.
    FlashCycle {
        /// First pool: WSOL -> intermediate mint.
        #[arg(long)]
        pool_in: String,
        /// Second pool: intermediate mint -> WSOL.
        #[arg(long)]
        pool_out: String,
        /// Borrow amount in lamports (WSOL has 9 decimals).
        #[arg(long)]
        amount: u64,
        /// Simulate the whole bundle through Jito instead of the RPC.
        #[arg(long)]
        jito: bool,
        /// Mechanics probe: allow an unprofitable cycle, prefund the expected
        /// loss from the wallet, and simulate it. This never sends a bundle.
        #[arg(long)]
        probe: bool,
        #[arg(long)]
        json: bool,
    },
    /// Build and simulate one Raydium AMM v4 swap on mainnet.
    ///
    /// Simulation only: nothing is sent. The command wraps a small amount of
    /// SOL into the wallet's WSOL account and swaps it in the given pool, which
    /// proves the instruction id, the account list and the quoted minimum
    /// against the live program.
    AmmV4Probe {
        /// AMM v4 pool address.
        #[arg(long)]
        pool: String,
        /// Input amount in lamports (the pool's base mint is the input).
        #[arg(long, default_value_t = 5_000_000)]
        amount: u64,
        /// Slippage tolerance applied to the quoted output, in basis points.
        #[arg(long, default_value_t = 50)]
        slippage_bps: u16,
        #[arg(long)]
        json: bool,
    },
    /// Quote and simulate one WSOL -> token swap on an Orca Whirlpool against
    /// mainnet. Reports the local quote and the output vault's actual gain; a
    /// non-zero quote error means the local pricer is not exact.
    WhirlpoolProbe {
        /// Whirlpool pool address.
        #[arg(long)]
        pool: String,
        /// Input amount in lamports.
        #[arg(long, default_value_t = 5_000_000)]
        amount: u64,
        /// Slippage tolerance applied to the quoted output, in basis points.
        #[arg(long, default_value_t = 50)]
        slippage_bps: u16,
        #[arg(long)]
        json: bool,
    },
    /// Quote and simulate one WSOL -> token swap on a Raydium CLMM pool against
    /// mainnet. Reports the local quote and the output vault's actual gain; a
    /// non-zero quote error means the local pricer is not exact.
    ClmmProbe {
        /// CLMM pool address.
        #[arg(long)]
        pool: String,
        /// Input amount in lamports.
        #[arg(long, default_value_t = 5_000_000)]
        amount: u64,
        /// Slippage tolerance applied to the quoted output, in basis points.
        #[arg(long, default_value_t = 50)]
        slippage_bps: u16,
        /// How many consecutive tick arrays to load in the swap direction.
        #[arg(long, default_value_t = 4)]
        arrays: usize,
        #[arg(long)]
        json: bool,
    },
    /// Prove end-to-end that Jito accepts a bundle. Sends a single transaction
    /// whose only instruction is the tip, prints the raw answer of every
    /// configured block engine, then polls until the bundle lands.
    JitoProbe {
        /// Tip paid to the selected Jito tip account, in lamports.
        #[arg(long, default_value_t = 10_000)]
        tip_lamports: u64,
        /// How long to watch the bundle before giving up.
        #[arg(long, default_value_t = 60)]
        wait_secs: u64,
        #[arg(long)]
        json: bool,
    },
    /// Create an address lookup table carrying the flash provider's stable
    /// accounts. Some provider lifecycles (Marginfi + CPMM) exceed the
    /// 1232-byte transaction limit without one. Simulates by default; sends
    /// only with both --real and --confirm.
    FlashAlt {
        /// Sign and send the setup transaction (requires runtime.real=true).
        #[arg(long)]
        real: bool,
        /// Required together with --real before anything is sent.
        #[arg(long)]
        confirm: bool,
        #[arg(long)]
        json: bool,
    },
    /// Deactivate or close a flash lookup table and recover its rent.
    FlashAltClose {
        /// Lookup table address (the value of `flash.lookup_table`).
        #[arg(long)]
        table: String,
        /// Sign and send the deactivate/close transaction.
        #[arg(long)]
        real: bool,
        /// Required together with --real before anything is sent.
        #[arg(long)]
        confirm: bool,
        #[arg(long)]
        json: bool,
    },
    /// Run the flash-loan arbitrage driver: scan, price and execute automatically.
    ///
    /// The driver holds a set of WSOL pairs, refreshes every pool on-chain,
    /// prices both directions of each two-pool cycle with Raydium's exact fee
    /// formula, and executes a cycle only when its on-chain minimums still
    /// clear the principal, the flash fee, the transaction costs, the tip and
    /// the configured minimum profit. A TUI is the default surface; --scan-only
    /// prints a single text report instead.
    Arb {
        /// Print one text report per scan instead of the TUI.
        #[arg(long)]
        scan_only: bool,
        /// Milliseconds between scans.
        #[arg(long, default_value_t = 750)]
        refresh_ms: u64,
        /// Minimum profit in lamports a cycle must clear to be executed.
        #[arg(long, default_value_t = 20_000)]
        min_profit_lamports: u64,
        /// Extra mints to pair against WSOL. Repeatable; the built-in set is
        /// always included.
        #[arg(long = "mint")]
        mints: Vec<String>,
        /// Replace the default borrow-size ladder with one amount in lamports.
        #[arg(long = "amount")]
        amounts: Vec<u64>,
        /// Execute cleared cycles instead of only reporting them. Requires
        /// runtime.real = true; without it the driver stays read-only.
        #[arg(long)]
        real: bool,
        /// Also price three-mint triangles through Jupiter. Off by default:
        /// an aggregator's quotes are mutually consistent, so a triangle is
        /// always negative by its fees, and the public rate limit makes the
        /// whole scan far slower.
        #[arg(long)]
        jupiter_triangles: bool,
        /// Scan once, then build and simulate the best cycle, printing what the
        /// chain said. Never submits. This is the way to check whether a
        /// reported cycle is real without risking a transaction.
        #[arg(long)]
        explain: bool,
        /// How long a discovered pool set stays valid, in milliseconds.
        /// Balances and tick arrays are always re-read; only the list of pools
        /// is cached, because `getProgramAccounts` is what free tiers throttle.
        #[arg(long, default_value_t = 300_000)]
        pool_ttl_ms: u64,
    },
    /// Quote a swap through Jupiter.
    Quote {
        #[arg(long)]
        input_mint: String,
        #[arg(long)]
        output_mint: String,
        #[arg(long)]
        amount: u64,
        #[arg(long, default_value_t = 50)]
        slippage_bps: u16,
    },
    /// Diagnose config, RPC, store, wallet, guard and Jupiter.
    Doctor {
        #[arg(long)]
        json: bool,
        /// Skip network probes.
        #[arg(long)]
        no_network: bool,
    },
    /// Scan or reclaim rent from the wallet's own empty token accounts.
    Rent {
        /// Explicitly allow signing and sending transactions.
        #[arg(long)]
        real: bool,
        /// Required together with --real to send.
        #[arg(long)]
        confirm: bool,
        #[arg(long)]
        json: bool,
    },
    /// Print a radar/settlement report from the local store.
    Report {
        /// Write markdown to this file instead of stdout.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Also write a CSV strategy summary.
        #[arg(long)]
        csv: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Write a commented default config.toml.
    Init {
        #[arg(long)]
        force: bool,
    },
    /// Append a named RPC provider endpoint block to config.toml.
    Rpc {
        #[command(subcommand)]
        action: RpcCommand,
    },
}

#[derive(Debug, Subcommand)]
enum RpcCommand {
    /// Add one provider endpoint (`beast rpc add --provider helius --key-env HELIUS_API_KEY --role exec`).
    Add {
        /// Provider preset: helius | alchemy | quicknode | chainstack | public
        #[arg(long, default_value = "helius")]
        provider: String,
        /// Environment variable holding the API key. Leave empty for `public`.
        #[arg(long)]
        key_env: Option<String>,
        /// feed | exec | discovery | rent | fallback
        #[arg(long, default_value = "feed")]
        role: String,
        /// Endpoint name; defaults to the provider id.
        #[arg(long)]
        name: Option<String>,
        /// Append even if an endpoint with this name already exists.
        #[arg(long)]
        force: bool,
    },
    /// Print every configured endpoint with its role and limits.
    List {
        #[arg(long)]
        json: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Run {
            real,
            feed_interval_ms,
            radar_interval_ms,
        } => {
            engine::run(engine::RunOptions {
                config_path: cli.config,
                real,
                json_logs: cli.json_logs,
                feed_interval_ms,
                radar_interval_ms,
            })
            .await
        }
        Command::Check { real, json } => {
            init_logging("warn", cli.json_logs)?;
            commands::check(cli.config.as_deref(), real, json)
        }
        Command::Venues { json, kind } => commands::venues(json, kind.as_deref()),
        Command::Discover {
            venue,
            mint,
            limit,
            save,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::discover(cli.config.as_deref(), &venue, &mint, limit, save, json).await
        }
        Command::FlashCycle {
            pool_in,
            pool_out,
            amount,
            jito,
            probe,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::flash_cycle(
                cli.config.as_deref(),
                &pool_in,
                &pool_out,
                amount,
                jito,
                json,
                probe,
            )
            .await
        }
        Command::AmmV4Probe {
            pool,
            amount,
            slippage_bps,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::amm_v4_probe(cli.config.as_deref(), &pool, amount, slippage_bps, json).await
        }
        Command::WhirlpoolProbe {
            pool,
            amount,
            slippage_bps,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::whirlpool_probe(cli.config.as_deref(), &pool, amount, slippage_bps, json).await
        }
        Command::ClmmProbe {
            pool,
            amount,
            slippage_bps,
            arrays,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::clmm_probe(
                cli.config.as_deref(),
                &pool,
                amount,
                slippage_bps,
                arrays,
                json,
            )
            .await
        }
        Command::JitoProbe {
            tip_lamports,
            wait_secs,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::jito_probe(cli.config.as_deref(), tip_lamports, wait_secs, json).await
        }
        Command::FlashAlt {
            real,
            confirm,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::flash_alt(cli.config.as_deref(), real, confirm, json).await
        }
        Command::FlashAltClose {
            table,
            real,
            confirm,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::flash_alt_close(cli.config.as_deref(), &table, real, confirm, json).await
        }
        Command::Arb {
            scan_only,
            refresh_ms,
            min_profit_lamports,
            mints,
            amounts,
            real,
            jupiter_triangles,
            explain,
            pool_ttl_ms,
        } => {
            init_logging(if scan_only && !explain { "warn" } else { "error" }, cli.json_logs)?;
            let mut options = crate::arb::ArbOptions {
                refresh_ms,
                min_profit_lamports,
                mints: {
                    if let Ok(env) = std::env::var("BEAST_MINTS") {
                        env.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        let mut defaults: Vec<String> = crate::arb::DEFAULT_PAIR_MINTS
                            .iter()
                            .map(|mint| (*mint).to_string())
                            .collect();
                        for mint in mints {
                            if !defaults.contains(&mint) {
                                defaults.push(mint);
                            }
                        }
                        defaults
                    }
                },
                amount_ladder: if amounts.is_empty() {
                    crate::arb::ArbOptions::default().amount_ladder
                } else {
                    amounts
                },
                scan_only,
                jupiter: None,
                mode: beast_core::types::Mode::Canary,
                jupiter_triangles,
                explain,
                discovery_ttl: std::time::Duration::from_millis(pool_ttl_ms),
            };
            options.mints.retain(|mint| !mint.trim().is_empty());
            crate::arb::arb(cli.config.as_deref(), real, options).await
        }
        Command::Quote {
            input_mint,
            output_mint,
            amount,
            slippage_bps,
        } => {
            init_logging("warn", cli.json_logs)?;
            commands::quote(
                cli.config.as_deref(),
                &input_mint,
                &output_mint,
                amount,
                slippage_bps,
            )
            .await
        }
        Command::Doctor { json, no_network } => {
            init_logging("warn", cli.json_logs)?;
            commands::doctor(cli.config.as_deref(), json, no_network).await
        }
        Command::Rent {
            real,
            confirm,
            json,
        } => {
            init_logging("info", cli.json_logs)?;
            commands::rent(cli.config.as_deref(), real, confirm, json).await
        }
        Command::Report { out, csv, json } => {
            init_logging("warn", cli.json_logs)?;
            commands::report(cli.config.as_deref(), out.as_deref(), csv.as_deref(), json)
        }
        Command::Init { force } => {
            init_logging("warn", cli.json_logs)?;
            commands::init(force)
        }
        Command::Rpc { action } => match action {
            RpcCommand::Add {
                provider,
                key_env,
                role,
                name,
                force,
            } => commands::rpc_add(
                cli.config.as_deref(),
                &provider,
                key_env.as_deref(),
                &role,
                name.as_deref(),
                force,
            ),
            RpcCommand::List { json } => commands::rpc_list(cli.config.as_deref(), json),
        },
    }
}

pub fn init_logging(level: &str, json: bool) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level));
    // Diagnostics belong on stderr so machine-readable command output stays
    // pure JSON even when INFO logging is enabled.
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr);
    let result = if json {
        builder.json().try_init()
    } else {
        builder.try_init()
    };
    result.map_err(|error| anyhow::anyhow!("cannot initialise logging: {error}"))?;
    Ok(())
}
