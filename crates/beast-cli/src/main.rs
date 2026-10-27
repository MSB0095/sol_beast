//! `beast` command line.

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
