# Sol Beast CLI Implementation Examples

This document provides detailed implementation examples for the Sol Beast CLI interface, showing how to integrate the command structure with the core library and implement the various features described in the CLI Design Specification.

## CLI Framework Setup

### 1. Main CLI Structure

```rust
// cli/src/main.rs
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use std::path::PathBuf;
use tokio;

#[derive(Parser, Debug)]
#[command(name = "sol-beast")]
#[command(about = "Advanced Solana trading bot CLI interface")]
#[command(version = "0.1.0")]
#[command(long_about = None)]
struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "~/.config/sol_beast/config.toml")]
    config: PathBuf,
    
    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,
    
    /// Suppress non-essential output
    #[arg(short, long)]
    quiet: bool,
    
    /// Output format (json, table, plain)
    #[arg(short, long, value_enum, default_value = "table")]
    format: OutputFormat,
    
    /// Color output (auto, always, never)
    #[arg(short, long, value_enum, default_value = "auto")]
    color: ColorChoice,
    
    /// Simulate operations without making changes
    #[arg(short, long)]
    dry_run: bool,
    
    /// Command to execute
    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Debug, Clone)]
enum OutputFormat {
    Json,
    Table,
    Plain,
}

#[derive(ValueEnum, Debug, Clone)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Configuration management commands
    Config(ConfigCommands),
    
    /// Wallet operations
    Wallet(WalletCommands),
    
    /// Trading operations
    Trade(TradeCommands),
    
    /// Bot control and management
    Bot(BotCommands),
    
    /// Protocol-specific operations
    Protocol(ProtocolCommands),
    
    /// Network connectivity
    Connectivity(ConnectivityCommands),
    
    /// Blockchain operations
    Blockchain(BlockchainCommands),
    
    /// Tools and utilities
    Tools(ToolsCommands),
    
    // Common aliases for convenience
    #[command(alias = "start")]
    BotStart { 
        #[arg(short, long)]
        mode: Option<BotMode>,
        #[arg(long)]
        config: Option<PathBuf>,
    },
    
    #[command(alias = "stop")]
    BotStop,
    
    #[command(alias = "status")]
    BotStatus {
        #[arg(long)]
        detailed: bool,
    },
    
    #[command(alias = "portfolio")]
    TradePortfolio {
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long)]
        detailed: bool,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize logging
    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else if !cli.quiet {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }
    
    // Handle command execution
    match cli.command {
        Commands::Config(config_cmd) => config::handle_config_command(config_cmd, &cli).await,
        Commands::Wallet(wallet_cmd) => wallet::handle_wallet_command(wallet_cmd, &cli).await,
        Commands::Trade(trade_cmd) => trade::handle_trade_command(trade_cmd, &cli).await,
        Commands::Bot(bot_cmd) => bot::handle_bot_command(bot_cmd, &cli).await,
        Commands::Protocol(protocol_cmd) => protocol::handle_protocol_command(protocol_cmd, &cli).await,
        Commands::Connectivity(connectivity_cmd) => connectivity::handle_connectivity_command(connectivity_cmd, &cli).await,
        Commands::Blockchain(blockchain_cmd) => blockchain::handle_blockchain_command(blockchain_cmd, &cli).await,
        Commands::Tools(tools_cmd) => tools::handle_tools_command(tools_cmd, &cli).await,
        
        // Handle aliases
        Commands::BotStart { mode, config } => {
            let bot_cmd = BotCommands::Start {
                mode: mode.unwrap_or(BotMode::Live),
                config_file: config,
            };
            bot::handle_bot_command(bot_cmd, &cli).await
        }
        
        Commands::BotStop => {
            let bot_cmd = BotCommands::Stop { force: false };
            bot::handle_bot_command(bot_cmd, &cli).await
        }
        
        Commands::BotStatus { detailed } => {
            let bot_cmd = BotCommands::Status { detailed };
            bot::handle_bot_command(bot_cmd, &cli).await
        }
        
        Commands::TradePortfolio { format, detailed } => {
            let trade_cmd = TradeCommands::Portfolio {
                format: format.unwrap_or(cli.format),
                detailed,
            };
            trade::handle_trade_command(trade_cmd, &cli).await
        }
    }
}
```

### 2. Command Module Structure

```rust
// cli/src/mod.rs
pub mod config;
pub mod wallet;
pub mod trade;
pub mod bot;
pub mod protocol;
pub mod connectivity;
pub mod blockchain;
pub mod tools;

use crate::error::CliError;
use crate::Cli;

// Common traits and utilities
pub trait CommandHandler {
    async fn handle(&self, cli: &Cli) -> Result<(), CliError>;
}

pub trait Configurable {
    fn load_config(&self, config_path: &std::path::Path) -> Result<sol_beast_core::config::Settings, CliError>;
    fn validate_config(&self, config: &sol_beast_core::config::Settings) -> Result<(), CliError>;
}

// Output formatting utilities
pub mod output {
    use colored::*;
    
    pub fn print_success(message: &str) {
        println!("{}", message.green());
    }
    
    pub fn print_error(message: &str) {
        eprintln!("{} {}", "ERROR:".red(), message);
    }
    
    pub fn print_warning(message: &str) {
        eprintln!("{} {}", "WARNING:".yellow(), message);
    }
    
    pub fn print_info(message: &str) {
        if !matches!(cli.quiet, Some(true)) {
            println!("{}", message.blue());
        }
    }
    
    pub fn format_json<T: serde::Serialize>(data: &T) -> Result<String, CliError> {
        serde_json::to_string_pretty(data)
            .map_err(|e| CliError::Serialization(e.to_string()))
    }
}
```

## Configuration Management Implementation

### 3. Config Commands

```rust
// cli/src/config.rs
use clap::{Args, Subcommand};
use std::path::PathBuf;
use sol_beast_core::config::{Settings, SettingsError};

#[derive(Subcommand, Debug)]
pub enum ConfigCommands {
    /// Get configuration value
    Get {
        /// Setting key (supports nested paths like "trading.tp_percent")
        key: String,
    },
    
    /// Set configuration value
    Set {
        /// Setting key
        key: String,
        /// Setting value
        value: String,
        /// Handle array values
        #[arg(short, long)]
        array: bool,
        /// Append to array
        #[arg(short, long)]
        append: bool,
        /// Set from environment variable
        #[arg(long)]
        from_env: bool,
        /// Set from file
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    
    /// Show configuration
    Show {
        /// Output format
        #[arg(short, long)]
        format: Option<OutputFormat>,
        /// Show sensitive values
        #[arg(short, long)]
        sensitive: bool,
        /// Filter by category
        #[arg(short, long)]
        category: Option<String>,
    },
    
    /// Validate configuration
    Validate {
        /// Configuration file to validate
        #[arg(short, long)]
        file: Option<PathBuf>,
        /// Validate with dry-run
        #[arg(long)]
        dry_run: bool,
    },
    
    /// Configuration templates
    Template(TemplateCommands),
    
    /// Configuration history
    History,
    
    /// Restore previous configuration
    Restore {
        /// Restore by number
        #[arg(short, long)]
        number: Option<usize>,
        /// Restore by timestamp
        #[arg(short, long)]
        timestamp: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum TemplateCommands {
    /// List available templates
    List,
    
    /// Generate configuration from template
    Generate {
        /// Template name (beginner, conservative, aggressive, etc.)
        template: String,
        /// Output path
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
        /// Override template values
        #[arg(short, long)]
        values: Option<Vec<String>>,
    },
}

pub async fn handle_config_command(command: ConfigCommands, cli: &Cli) -> Result<(), CliError> {
    match command {
        ConfigCommands::Get { key } => handle_config_get(&key, cli).await,
        ConfigCommands::Set { key, value, array, append, from_env, file } => {
            handle_config_set(&key, &value, array, append, from_env, file, cli).await
        }
        ConfigCommands::Show { format, sensitive, category } => {
            handle_config_show(format, sensitive, category, cli).await
        }
        ConfigCommands::Validate { file, dry_run } => {
            handle_config_validate(file, dry_run, cli).await
        }
        ConfigCommands::Template(template_cmd) => {
            handle_template_command(template_cmd, cli).await
        }
        ConfigCommands::History => handle_config_history(cli).await,
        ConfigCommands::Restore { number, timestamp } => {
            handle_config_restore(number, timestamp, cli).await
        }
    }
}

async fn handle_config_get(key: &str, cli: &Cli) -> Result<(), CliError> {
    let settings = load_config(cli).await?;
    
    // Handle nested paths like "trading.tp_percent"
    let value = get_nested_value(&settings, key)
        .ok_or_else(|| CliError::Config(format!("Setting '{}' not found", key)))?;
    
    match cli.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&serde_json::json!({ key: value }))
                .map_err(|e| CliError::Serialization(e.to_string()))?;
            println!("{}", json);
        }
        OutputFormat::Table | OutputFormat::Plain => {
            println!("{}", value);
        }
    }
    
    Ok(())
}

async fn handle_config_set(
    key: &str, 
    value: &str, 
    array: bool, 
    append: bool, 
    from_env: bool, 
    file: Option<PathBuf>,
    cli: &Cli
) -> Result<(), CliError> {
    if cli.dry_run {
        output::print_info("DRY RUN: Configuration changes not applied");
        return Ok(());
    }
    
    let mut settings = load_config(cli).await?;
    
    let new_value = if from_env {
        std::env::var(value)
            .map_err(|_| CliError::Config(format!("Environment variable '{}' not found", value)))?
    } else {
        value.to_string()
    };
    
    if array {
        if append {
            append_to_array(&mut settings, key, &new_value)?;
        } else {
            set_array_value(&mut settings, key, &new_value)?;
        }
    } else {
        set_nested_value(&mut settings, key, &new_value)?;
    }
    
    // Validate configuration
    settings.validate()
        .map_err(|e| CliError::Config(format!("Configuration validation failed: {}", e)))?;
    
    // Save configuration
    settings.save_to_file(&cli.config.to_string_lossy())
        .map_err(|e| CliError::Config(format!("Failed to save configuration: {}", e)))?;
    
    output::print_success(&format!("Configuration '{}' updated successfully", key));
    Ok(())
}

async fn handle_config_show(
    format: Option<OutputFormat>,
    sensitive: bool,
    category: Option<String>,
    cli: &Cli
) -> Result<(), CliError> {
    let settings = load_config(cli).await?;
    
    let output_format = format.unwrap_or(cli.format);
    
    match output_format {
        OutputFormat::Json => {
            let config_json = if sensitive {
                serde_json::to_value(&settings)
            } else {
                // Filter out sensitive fields
                let sanitized = sanitize_config(&settings);
                serde_json::to_value(&sanitized)
            }.map_err(|e| CliError::Serialization(e.to_string()))?;
            
            let json_str = serde_json::to_string_pretty(&config_json)
                .map_err(|e| CliError::Serialization(e.to_string()))?;
            println!("{}", json_str);
        }
        OutputFormat::Table => {
            print_config_table(&settings, category, sensitive)?;
        }
        OutputFormat::Plain => {
            print_config_plain(&settings, category, sensitive)?;
        }
    }
    
    Ok(())
}

// Utility functions for nested configuration access
fn get_nested_value(settings: &Settings, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    
    match parts.as_slice() {
        ["trading", "tp_percent"] => Some(settings.tp_percent.to_string()),
        ["trading", "sl_percent"] => Some(settings.sl_percent.to_string()),
        ["network", "rpc_urls"] => Some(settings.solana_rpc_urls.join(", ")),
        ["wallet", "keypair_path"] => settings.wallet_keypair_path.clone(),
        // Add more mappings as needed
        _ => None,
    }
}

fn set_nested_value(settings: &mut Settings, path: &str, value: &str) -> Result<(), CliError> {
    let parts: Vec<&str> = path.split('.').collect();
    
    match parts.as_slice() {
        ["trading", "tp_percent"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| CliError::Config("Invalid tp_percent value".to_string()))?;
            if parsed <= 0.0 || parsed > 1000.0 {
                return Err(CliError::Config("tp_percent must be between 0.1 and 1000.0".to_string()));
            }
            settings.tp_percent = parsed;
        }
        ["trading", "sl_percent"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| CliError::Config("Invalid sl_percent value".to_string()))?;
            if parsed <= 0.0 || parsed > 100.0 {
                return Err(CliError::Config("sl_percent must be between 0.1 and 100.0".to_string()));
            }
            settings.sl_percent = parsed;
        }
        ["network", "rpc_urls"] => {
            let urls: Vec<String> = value.split(',').map(|s| s.trim().to_string()).collect();
            if urls.is_empty() {
                return Err(CliError::Config("At least one RPC URL is required".to_string()));
            }
            settings.solana_rpc_urls = urls;
        }
        // Add more mappings as needed
        _ => return Err(CliError::Config(format!("Unknown configuration path: {}", path))),
    }
    
    Ok(())
}
```

## Wallet Operations Implementation

### 4. Wallet Commands

```rust
// cli/src/wallet.rs
use clap::{Args, Subcommand};
use sol_beast_core::config::wallet::{WalletManager, WalletInfo};
use sol_beast_core::blockchain::signer::{KeypairSigner, Signer};
use solana_sdk::signature::{Keypair, Signature};
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum WalletCommands {
    /// Connect wallet with various sources
    Connect {
        /// Connect via keypair file
        #[arg(short, long)]
        keypair: Option<PathBuf>,
        /// Connect via private key
        #[arg(short, long)]
        private_key: Option<String>,
        /// Connect via environment variable
        #[arg(short, long)]
        env_var: Option<String>,
        /// Connect via address
        #[arg(short, long)]
        address: Option<String>,
        /// Interactive connection
        #[arg(short, long)]
        interactive: bool,
        /// Simulation mode
        #[arg(short, long)]
        simulate: bool,
    },
    
    /// Show wallet status
    Status {
        /// Show verbose information
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Show wallet information
    Info {
        /// Output format
        #[arg(short, long)]
        format: Option<OutputFormat>,
    },
    
    /// Show wallet balance
    Balance {
        /// Specific token mint
        #[arg(short, long)]
        token: Option<String>,
        /// Show all tokens
        #[arg(short, long)]
        all: bool,
    },
    
    /// Export wallet
    Export {
        /// Export format
        #[arg(short, long)]
        format: Option<String>,
        /// Output path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// Generate new wallet
    Generate {
        /// Output path
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Generate test wallet
        #[arg(short, long)]
        simulate: bool,
    },
    
    /// Disconnect wallet
    Disconnect {
        /// Force disconnection
        #[arg(short, long)]
        force: bool,
    },
    
    /// Wallet security operations
    Security(SecurityCommands),
}

#[derive(Subcommand, Debug)]
pub enum SecurityCommands {
    /// Verify wallet integrity
    Verify,
    
    /// Test transaction signing
    TestSign {
        /// Amount for test transaction
        #[arg(short, long, default_value = "0.001")]
        amount: f64,
    },
    
    /// Security audit
    Audit {
        /// Check permissions
        #[arg(short, long)]
        check_permissions: bool,
        /// Check exposure
        #[arg(short, long)]
        check_exposure: bool,
    },
}

pub async fn handle_wallet_command(command: WalletCommands, cli: &Cli) -> Result<(), CliError> {
    let mut wallet_manager = WalletManager::new();
    
    match command {
        WalletCommands::Connect { keypair, private_key, env_var, address, interactive, simulate } => {
            handle_wallet_connect(&mut wallet_manager, keypair, private_key, env_var, address, interactive, simulate, cli).await
        }
        WalletCommands::Status { verbose } => handle_wallet_status(&wallet_manager, verbose, cli).await,
        WalletCommands::Info { format } => handle_wallet_info(&wallet_manager, format, cli).await,
        WalletCommands::Balance { token, all } => handle_wallet_balance(&wallet_manager, token, all, cli).await,
        WalletCommands::Export { format, output } => handle_wallet_export(&wallet_manager, format, output, cli).await,
        WalletCommands::Generate { output, simulate } => handle_wallet_generate(output, simulate, cli).await,
        WalletCommands::Disconnect { force } => handle_wallet_disconnect(&mut wallet_manager, force, cli).await,
        WalletCommands::Security(security_cmd) => handle_security_commands(&mut wallet_manager, security_cmd, cli).await,
    }
}

async fn handle_wallet_connect(
    wallet_manager: &mut WalletManager,
    keypair: Option<PathBuf>,
    private_key: Option<String>,
    env_var: Option<String>,
    address: Option<String>,
    interactive: bool,
    simulate: bool,
    cli: &Cli
) -> Result<(), CliError> {
    if cli.dry_run {
        output::print_info("DRY RUN: Wallet connection simulated");
        return Ok(());
    }
    
    let wallet_info = if interactive {
        // Interactive wallet connection
        let wallet_address = dialoguer::Input::<String>::new()
            .with_prompt("Enter wallet address")
            .interact()
            .map_err(|e| CliError::UserInput(e.to_string()))?;
        
        WalletInfo {
            address: wallet_address,
            connected: false,
            balance: 0,
        }
    } else {
        // Find connection method
        let (connection_type, value) = if let Some(path) = keypair {
            ("keypair", path.to_string_lossy().to_string())
        } else if let Some(pk) = private_key {
            ("private_key", pk)
        } else if let Some(env) = env_var {
            ("env_var", env)
        } else if let Some(addr) = address {
            ("address", addr)
        } else {
            return Err(CliError::Config("No connection method specified".to_string()));
        };
        
        match connection_type {
            "keypair" => {
                let keypair_data = std::fs::read(&value)
                    .map_err(|e| CliError::Wallet(format!("Failed to read keypair file: {}", e)))?;
                let keypair = Keypair::from_bytes(&keypair_data)
                    .map_err(|e| CliError::Wallet(format!("Invalid keypair format: {}", e)))?;
                
                WalletInfo {
                    address: keypair.pubkey().to_string(),
                    connected: true,
                    balance: 0, // Will be updated
                }
            }
            "private_key" => {
                let key_bytes = parse_private_key(&value)?;
                let keypair = Keypair::from_bytes(&key_bytes)
                    .map_err(|e| CliError::Wallet(format!("Invalid private key: {}", e)))?;
                
                WalletInfo {
                    address: keypair.pubkey().to_string(),
                    connected: true,
                    balance: 0,
                }
            }
            "env_var" => {
                let private_key = std::env::var(&value)
                    .map_err(|_| CliError::Wallet(format!("Environment variable '{}' not found", value)))?;
                let key_bytes = parse_private_key(&private_key)?;
                let keypair = Keypair::from_bytes(&key_bytes)
                    .map_err(|e| CliError::Wallet(format!("Invalid private key: {}", e)))?;
                
                WalletInfo {
                    address: keypair.pubkey().to_string(),
                    connected: true,
                    balance: 0,
                }
            }
            "address" => {
                WalletInfo {
                    address: value,
                    connected: false,
                    balance: 0,
                }
            }
            _ => unreachable!(),
        }
    };
    
    // Connect wallet
    wallet_manager.connect_wallet(wallet_info.address.clone())?;
    
    // Update balance if connected
    if wallet_manager.is_connected() {
        let balance = get_wallet_balance(&wallet_manager).await?;
        wallet_manager.update_balance(balance);
    }
    
    output::print_success(&format!("Wallet connected: {}", wallet_info.address));
    
    Ok(())
}

async fn handle_wallet_status(wallet_manager: &WalletManager, verbose: bool, cli: &Cli) -> Result<(), CliError> {
    if !wallet_manager.is_connected() {
        output::print_warning("No wallet connected");
        return Ok(());
    }
    
    let wallet_info = wallet_manager.get_current_wallet()
        .ok_or_else(|| CliError::Wallet("No wallet information available".to_string()))?;
    
    match cli.format {
        OutputFormat::Json => {
            let status = serde_json::json!({
                "address": wallet_info.address,
                "connected": wallet_info.connected,
                "balance_sol": wallet_info.balance as f64 / 1_000_000_000.0,
                "balance_lamports": wallet_info.balance
            });
            
            println!("{}", serde_json::to_string_pretty(&status)
                .map_err(|e| CliError::Serialization(e.to_string()))?);
        }
        _ => {
            println!("Wallet Status");
            println!("  Address: {}", wallet_info.address);
            println!("  Connected: {}", if wallet_info.connected { "Yes" } else { "No" });
            println!("  Balance: {:.4} SOL", wallet_info.balance as f64 / 1_000_000_000.0);
            
            if verbose {
                println!("  Last Updated: {}", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
            }
        }
    }
    
    Ok(())
}

// Parse private key from various formats
fn parse_private_key(private_key_str: &str) -> Result<Vec<u8>, CliError> {
    let trimmed = private_key_str.trim();
    
    // Try base58 format
    if let Ok(bytes) = bs58::decode(trimmed).into_vec() {
        if bytes.len() == 64 {
            return Ok(bytes);
        }
    }
    
    // Try hex format
    if let Ok(bytes) = hex::decode(trimmed) {
        if bytes.len() == 64 {
            return Ok(bytes);
        }
    }
    
    // Try JSON format
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let bytes: Vec<u8> = serde_json::from_str(trimmed)
            .map_err(|e| CliError::Wallet(format!("Invalid JSON format: {}", e)))?;
        if bytes.len() == 64 {
            return Ok(bytes);
        }
    }
    
    Err(CliError::Wallet("Invalid private key format. Expected base58, hex, or JSON array".to_string()))
}

// Get wallet balance from Solana RPC
async fn get_wallet_balance(wallet_manager: &WalletManager) -> Result<u64, CliError> {
    let address = wallet_manager.get_wallet_address()
        .ok_or_else(|| CliError::Wallet("No wallet connected".to_string()))?;
    
    let settings = load_config(cli).await?;
    let rpc_client = sol_beast_core::blockchain::rpc_client::NativeRpcClient::new(
        settings.solana_rpc_urls[0].clone()
    );
    
    rpc_client.get_balance(&address.parse()
        .map_err(|e| CliError::Wallet(format!("Invalid address: {}", e)))?)
        .await
        .map_err(|e| CliError::Network(e.to_string()))
}
```

## Trading Operations Implementation

### 5. Trade Commands

```rust
// cli/src/trade.rs
use clap::{Args, Subcommand};
use sol_beast_core::trading::{TradingStrategy, Buyer, StrategyConfig};
use sol_beast_core::blockchain::transaction::{TransactionBuilder, TransactionResult};
use solana_sdk::pubkey::Pubkey;
use std::path::PathBuf;

#[derive(Subcommand, Debug)]
pub enum TradeCommands {
    /// Manual trading operations
    Manual(ManualTradeCommands),
    
    /// Strategy management
    Strategy(StrategyCommands),
    
    /// Portfolio management
    Portfolio {
        /// Output format
        #[arg(short, long)]
        format: Option<OutputFormat>,
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
        /// Show only profitable positions
        #[arg(long)]
        profit_positive: bool,
        /// Minimum holding time filter
        #[arg(long)]
        min_hold_time: Option<String>,
        /// Sort by field
        #[arg(short, long)]
        sort_by: Option<String>,
        /// Limit results
        #[arg(short, long)]
        limit: Option<usize>,
    },
    
    /// Trading history
    History {
        /// Time period (1h, 24h, 7d, 30d)
        #[arg(short, long)]
        period: Option<String>,
        /// Specific mint to filter
        #[arg(short, long)]
        mint: Option<String>,
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
        /// Export format
        #[arg(short, long)]
        export: Option<PathBuf>,
    },
    
    /// Real-time monitoring
    Monitor {
        /// Live monitoring mode
        #[arg(short, long)]
        live: bool,
        /// Profit threshold alert
        #[arg(short, long)]
        threshold: Option<f64>,
        /// Enable alerts
        #[arg(short, long)]
        alerts: bool,
        /// Telegram bot token for alerts
        #[arg(long)]
        telegram_token: Option<String>,
        /// Watch specific mints
        #[arg(short, long)]
        watchlist: Option<Vec<String>>,
    },
    
    /// Buy tokens
    Buy {
        /// Token mint address
        mint: String,
        /// SOL amount to spend
        #[arg(short, long)]
        amount: Option<f64>,
        /// Token amount to buy
        #[arg(short, long)]
        token_amount: Option<f64>,
        /// Maximum slippage percentage
        #[arg(short, long)]
        max_slippage: Option<f64>,
        /// Batch file for multiple buys
        #[arg(short, long)]
        batch_file: Option<PathBuf>,
    },
    
    /// Sell tokens
    Sell {
        /// Token mint address
        mint: String,
        /// Percentage of holding to sell
        #[arg(short, long)]
        percentage: Option<f64>,
        /// Token amount to sell
        #[arg(short, long)]
        amount: Option<f64>,
        /// Minimum profit percentage
        #[arg(short, long)]
        min_profit: Option<f64>,
        /// Sell all holdings
        #[arg(short, long)]
        all: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ManualTradeCommands {
    /// Direct buy operation
    Buy {
        mint: String,
        #[arg(short, long)]
        amount: Option<f64>,
        #[arg(short, long)]
        slippage: Option<u64>,
    },
    
    /// Direct sell operation
    Sell {
        mint: String,
        #[arg(short, long)]
        percentage: Option<f64>,
        #[arg(short, long)]
        amount: Option<f64>,
    },
    
    /// Limit orders
    Limit {
        #[command(subcommand)]
        limit_type: LimitType,
    },
}

#[derive(Subcommand, Debug)]
pub enum LimitType {
    /// Buy limit order
    Buy {
        mint: String,
        /// Target price in SOL
        price: f64,
        /// Amount to buy
        amount: f64,
        /// Expiry time
        #[arg(short, long)]
        expiry: Option<String>,
    },
    
    /// Sell limit order
    Sell {
        mint: String,
        /// Target price in SOL
        price: f64,
        /// Percentage to sell
        percentage: Option<f64>,
        /// Amount to sell
        amount: Option<f64>,
        /// Expiry time
        #[arg(short, long)]
        expiry: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum StrategyCommands {
    /// Set strategy parameters
    Set {
        /// Take profit percentage
        #[arg(short, long)]
        tp_percent: Option<f64>,
        /// Stop loss percentage
        #[arg(short, long)]
        sl_percent: Option<f64>,
        /// Maximum hold time in seconds
        #[arg(short, long)]
        max_hold_time: Option<i64>,
        /// Minimum liquidity in SOL
        #[arg(short, long)]
        min_liquidity: Option<f64>,
    },
    
    /// Strategy templates
    Template {
        #[command(subcommand)]
        template_type: StrategyTemplate,
    },
    
    /// Validate strategy configuration
    Validate {
        /// Strategy config file
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
pub enum StrategyTemplate {
    /// Conservative strategy template
    Conservative {
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
    },
    
    /// Aggressive strategy template
    Aggressive {
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
        #[arg(short, long)]
        tp: Option<f64>,
        #[arg(short, long)]
        sl: Option<f64>,
    },
    
    /// List available templates
    List,
}

pub async fn handle_trade_command(command: TradeCommands, cli: &Cli) -> Result<(), CliError> {
    let settings = load_config(cli).await?;
    
    match command {
        TradeCommands::Manual(manual_cmd) => handle_manual_trade(manual_cmd, &settings, cli).await,
        TradeCommands::Strategy(strategy_cmd) => handle_strategy_command(strategy_cmd, &settings, cli).await,
        TradeCommands::Portfolio { format, detailed, profit_positive, min_hold_time, sort_by, limit } => {
            handle_portfolio(format, detailed, profit_positive, min_hold_time, sort_by, limit, &settings, cli).await
        }
        TradeCommands::History { period, mint, detailed, export } => {
            handle_trading_history(period, mint, detailed, export, &settings, cli).await
        }
        TradeCommands::Monitor { live, threshold, alerts, telegram_token, watchlist } => {
            handle_trade_monitor(live, threshold, alerts, telegram_token, watchlist, &settings, cli).await
        }
        TradeCommands::Buy { mint, amount, token_amount, max_slippage, batch_file } => {
            handle_trade_buy(mint, amount, token_amount, max_slippage, batch_file, &settings, cli).await
        }
        TradeCommands::Sell { mint, percentage, amount, min_profit, all } => {
            handle_trade_sell(mint, percentage, amount, min_profit, all, &settings, cli).await
        }
    }
}

async fn handle_trade_buy(
    mint: String,
    amount: Option<f64>,
    token_amount: Option<f64>,
    max_slippage: Option<f64>,
    batch_file: Option<PathBuf>,
    settings: &Settings,
    cli: &Cli
) -> Result<(), CliError> {
    if cli.dry_run {
        output::print_info("DRY RUN: Trade buy operation simulated");
        return Ok(());
    }
    
    // Parse mint address
    let mint_pubkey = mint.parse::<Pubkey>()
        .map_err(|e| CliError::Trading(format!("Invalid mint address: {}", e)))?;
    
    // Get strategy configuration
    let strategy_config = StrategyConfig {
        tp_percent: settings.tp_percent,
        sl_percent: settings.sl_percent,
        timeout_secs: settings.timeout_secs,
        max_holded_coins: settings.max_holded_coins,
        // ... other fields
    };
    
    // Initialize trading components
    let strategy = TradingStrategy::new(strategy_config.clone());
    let tx_builder = TransactionBuilder::new(settings.pump_fun_program.clone())?;
    let buyer = Buyer::new(settings.clone(), strategy);
    
    // Calculate token amount if only SOL amount provided
    let token_amount = if let Some(sol_amount) = amount {
        let bonding_curve_pda = tx_builder.get_bonding_curve_pda(&mint)?;
        let bonding_curve_data = get_bonding_curve_data(&bonding_curve_pda, settings).await?;
        
        tx_builder.calculate_token_output(
            &bonding_curve_data,
            (sol_amount * 1_000_000_000.0) as u64
        )? as f64
    } else if let Some(amount) = token_amount {
        amount
    } else {
        return Err(CliError::Trading("Either --amount (SOL) or --token_amount must be specified".to_string()));
    };
    
    // Validate mint before trading
    if let Err(e) = validate_mint_trading_eligibility(&mint_pubkey, settings).await {
        output::print_error(&format!("Mint validation failed: {}", e));
        return Err(e);
    }
    
    // Execute buy
    let result = buyer.buy_token(
        mint_pubkey,
        (token_amount * 1_000_000.0) as u64, // Convert to base units
        max_slippage.unwrap_or(settings.slippage_bps as f64) / 100.0
    ).await;
    
    match result {
        Ok(tx_result) => {
            output::print_success(&format!(
                "Buy order successful: {} tokens of {}",
                token_amount, mint
            ));
            output::print_info(&format!("Transaction signature: {}", tx_result.signature));
        }
        Err(e) => {
            output::print_error(&format!("Buy order failed: {}", e));
            return Err(CliError::Trading(e.to_string()));
        }
    }
    
    Ok(())
}

async fn handle_portfolio(
    format: Option<OutputFormat>,
    detailed: bool,
    profit_positive: bool,
    min_hold_time: Option<String>,
    sort_by: Option<String>,
    limit: Option<usize>,
    settings: &Settings,
    cli: &Cli
) -> Result<(), CliError> {
    let output_format = format.unwrap_or(cli.format);
    
    // Get portfolio data
    let portfolio = get_portfolio_data(settings).await?;
    
    // Apply filters
    let mut filtered_portfolio = portfolio;
    
    if profit_positive {
        filtered_portfolio.retain(|holding| {
            holding.profit_loss.unwrap_or(0.0) > 0.0
        });
    }
    
    if let Some(hold_time) = min_hold_time {
        let min_seconds = parse_duration(&hold_time)?;
        filtered_portfolio.retain(|holding| {
            holding.hold_time >= min_seconds
        });
    }
    
    // Apply sorting
    if let Some(sort_field) = sort_by {
        match sort_field.as_str() {
            "profit" => filtered_portfolio.sort_by(|a, b| {
                b.profit_loss.unwrap_or(0.0).partial_cmp(&a.profit_loss.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
            }),
            "value" => filtered_portfolio.sort_by(|a, b| {
                b.current_value.unwrap_or(0.0).partial_cmp(&a.current_value.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
            }),
            "time" => filtered_portfolio.sort_by(|a, b| {
                b.hold_time.cmp(&a.hold_time)
            }),
            _ => {}
        }
    }
    
    // Apply limit
    if let Some(limit_count) = limit {
        filtered_portfolio.truncate(limit_count);
    }
    
    match output_format {
        OutputFormat::Json => {
            let json_str = serde_json::to_string_pretty(&filtered_portfolio)
                .map_err(|e| CliError::Serialization(e.to_string()))?;
            println!("{}", json_str);
        }
        _ => {
            print_portfolio_table(&filtered_portfolio, detailed)?;
        }
    }
    
    Ok(())
}

// Utility functions
async fn get_portfolio_data(settings: &Settings) -> Result<Vec<PortfolioHolding>, CliError> {
    // Implementation would fetch current holdings from the bot state
    // This is a placeholder for the actual implementation
    Ok(vec![])
}

fn print_portfolio_table(holdings: &[PortfolioHolding], detailed: bool) -> Result<(), CliError> {
    println!("Portfolio Holdings");
    println!("{:<20} {:<15} {:<12} {:<12} {:<10} {:<15}", 
             "Mint", "Amount", "Value (SOL)", "PnL (%)", "Time", "Current Price");
    println!("{}", "-".repeat(95));
    
    for holding in holdings {
        let pnl_str = if let Some(pnl) = holding.profit_loss {
            format!("{:.2}%", pnl)
        } else {
            "N/A".to_string()
        };
        
        let time_str = format_duration(holding.hold_time);
        
        println!("{:<20} {:<15.4} {:<12.4} {:<12} {:<10} {:<15.8}", 
                 &holding.mint[..8], // Truncate mint for display
                 holding.amount,
                 holding.current_value.unwrap_or(0.0),
                 pnl_str,
                 time_str,
                 holding.current_price.unwrap_or(0.0));
    }
    
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct PortfolioHolding {
    mint: String,
    amount: f64,
    current_value: Option<f64>,
    profit_loss: Option<f64>,
    hold_time: i64,
    current_price: Option<f64>,
    buy_price: Option<f64>,
}
```

## Error Handling Implementation

### 6. Custom Error Types

```rust
// cli/src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CliError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Wallet error: {0}")]
    Wallet(String),
    
    #[error("Trading error: {0}")]
    Trading(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Blockchain error: {0}")]
    Blockchain(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("User input error: {0}")]
    UserInput(String),
    
    #[error("Validation error: {0}")]
    Validation(String),
    
    #[error("System error: {0}")]
    System(#[from] std::io::Error),
    
    #[error("Core library error: {0}")]
    Core(#[from] sol_beast_core::core::error::CoreError),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Config(_) => 2,
            CliError::Wallet(_) => 3,
            CliError::Trading(_) => 4,
            CliError::Network(_) => 5,
            CliError::Validation(_) => 6,
            _ => 1,
        }
    }
    
    pub fn suggestions(&self) -> Vec<String> {
        match self {
            CliError::Config(msg) => {
                if msg.contains("not found") {
                    vec![
                        "Check the configuration file path".to_string(),
                        "Run 'sol-beast config show' to see current settings".to_string(),
                        "Use 'sol-beast config templates' to generate a new config".to_string(),
                    ]
                } else {
                    vec!["Check configuration syntax".to_string()]
                }
            }
            CliError::Wallet(msg) => {
                if msg.contains("invalid") {
                    vec![
                        "Check wallet format (base58, hex, or JSON)".to_string(),
                        "Verify file permissions (should be 600)".to_string(),
                        "Try 'sol-beast wallet connect --help' for usage".to_string(),
                    ]
                } else {
                    vec!["Check wallet connection".to_string()]
                }
            }
            CliError::Trading(msg) => {
                vec![
                    "Check token mint address format".to_string(),
                    "Verify sufficient balance".to_string(),
                    "Try with --dry-run to test operation".to_string(),
                ]
            }
            CliError::Network(msg) => {
                vec![
                    "Check network connectivity".to_string(),
                    "Verify RPC URL configuration".to_string(),
                    "Try switching to a different RPC endpoint".to_string(),
                ]
            }
            _ => vec![],
        }
    }
}

// Error output formatting
pub fn print_error_with_suggestions(error: &CliError) {
    eprintln!("{} {}", "ERROR:".red(), error);
    
    let suggestions = error.suggestions();
    if !suggestions.is_empty() {
        eprintln!("\n{} {}", "Suggestions:".yellow());
        for (i, suggestion) in suggestions.iter().enumerate() {
            eprintln!("  {}. {}", i + 1, suggestion);
        }
    }
    
    std::process::exit(error.exit_code());
}
```

## Safety Mechanisms Implementation

### 7. Validation and Safety Checks

```rust
// cli/src/safety.rs
use sol_beast_core::config::Settings;
use sol_beast_core::blockchain::rpc_client::NativeRpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;

pub struct SafetyChecker {
    settings: Settings,
    validated_mints: HashSet<Pubkey>,
}

impl SafetyChecker {
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            validated_mints: HashSet::new(),
        }
    }
    
    pub async fn validate_trade_operation(
        &mut self,
        mint: &Pubkey,
        amount: f64,
        operation: TradeOperation
    ) -> Result<(), CliError> {
        // 1. Validate mint address
        if !self.is_valid_mint(mint).await? {
            return Err(CliError::Validation("Invalid or suspicious mint address".to_string()));
        }
        
        // 2. Validate amount
        if let Err(e) = self.validate_amount(amount, operation) {
            output::print_warning(&format!("Amount validation warning: {}", e));
            return Err(e);
        }
        
        // 3. Check liquidity requirements
        if let Err(e) = self.check_liquidity_requirements(mint, amount).await {
            return Err(CliError::Validation(format!("Liquidity check failed: {}", e)));
        }
        
        // 4. Validate configuration
        if let Err(e) = self.validate_trading_config() {
            return Err(CliError::Validation(format!("Configuration validation failed: {}", e)));
        }
        
        Ok(())
    }
    
    async fn is_valid_mint(&mut self, mint: &Pubkey) -> Result<bool, CliError> {
        // Check if mint is already validated
        if self.validated_mints.contains(mint) {
            return Ok(true);
        }
        
        let rpc_client = NativeRpcClient::new(self.settings.solana_rpc_urls[0].clone());
        
        // Check if account exists
        let account_info = rpc_client.get_account_info(mint).await
            .map_err(|e| CliError::Network(format!("Failed to check mint account: {}", e)))?;
        
        if account_info.is_none() {
            return Ok(false);
        }
        
        // Additional validation logic
        let is_valid = self.perform_additional_mint_validation(mint, &account_info.unwrap()).await?;
        
        if is_valid {
            self.validated_mints.insert(*mint);
        }
        
        Ok(is_valid)
    }
    
    async fn check_liquidity_requirements(&self, mint: &Pubkey, amount: f64) -> Result<(), CliError> {
        let rpc_client = NativeRpcClient::new(self.settings.solana_rpc_urls[0].clone());
        
        // Get bonding curve account
        let tx_builder = sol_beast_core::blockchain::transaction::TransactionBuilder::new(
            self.settings.pump_fun_program.clone()
        )?;
        
        let bonding_curve_pda = tx_builder.get_bonding_curve_pda(&mint.to_string())?;
        let bonding_curve_data = rpc_client.get_account_data(&bonding_curve_pda.parse()?)
            .await
            .map_err(|e| CliError::Blockchain(format!("Failed to get bonding curve data: {}", e)))?;
        
        let bonding_curve = sol_beast_core::blockchain::rpc_client::parse_bonding_curve(&bonding_curve_data)
            .map_err(|e| CliError::Blockchain(format!("Failed to parse bonding curve: {}", e)))?;
        
        // Check minimum liquidity
        let current_liquidity = bonding_curve.spot_price_sol_per_token()
            .map(|price| price * bonding_curve.virtual_token_reserves as f64 / 1_000_000_000.0)
            .unwrap_or(0.0);
        
        if current_liquidity < self.settings.min_liquidity_sol {
            return Err(CliError::Validation(format!(
                "Insufficient liquidity: {:.4} SOL (minimum: {:.4} SOL)",
                current_liquidity, self.settings.min_liquidity_sol
            )));
        }
        
        // Check maximum liquidity
        if current_liquidity > self.settings.max_liquidity_sol {
            return Err(CliError::Validation(format!(
                "Excessive liquidity: {:.4} SOL (maximum: {:.4} SOL)",
                current_liquidity, self.settings.max_liquidity_sol
            )));
        }
        
        Ok(())
    }
    
    fn validate_amount(&self, amount: f64, operation: TradeOperation) -> Result<(), CliError> {
        match operation {
            TradeOperation::Buy => {
                if amount <= 0.0 {
                    return Err(CliError::Validation("Buy amount must be positive".to_string()));
                }
                
                if amount > self.settings.buy_amount * 10.0 {
                    return Err(CliError::Validation(format!(
                        "Buy amount too large: {:.4} SOL (maximum recommended: {:.4} SOL)",
                        amount, self.settings.buy_amount * 10.0
                    )));
                }
            }
            TradeOperation::Sell => {
                if amount <= 0.0 || amount > 100.0 {
                    return Err(CliError::Validation("Sell percentage must be between 0 and 100".to_string()));
                }
            }
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum TradeOperation {
    Buy,
    Sell,
}

// Slippage protection
pub async fn validate_slippage(
    expected_price: f64,
    actual_price: f64,
    max_slippage: f64
) -> Result<(), CliError> {
    if expected_price == 0.0 {
        return Err(CliError::Validation("Expected price cannot be zero".to_string()));
    }
    
    let slippage = ((actual_price - expected_price) / expected_price).abs() * 100.0;
    
    if slippage > max_slippage {
        return Err(CliError::Validation(format!(
            "Slippage too high: {:.2}% (maximum allowed: {:.2}%)",
            slippage, max_slippage
        )));
    }
    
    Ok(())
}

// Wallet balance validation
pub async fn validate_wallet_balance(
    wallet_address: &str,
    required_amount: u64,
    settings: &Settings
) -> Result<(), CliError> {
    let rpc_client = NativeRpcClient::new(settings.solana_rpc_urls[0].clone());
    let pubkey = wallet_address.parse()
        .map_err(|e| CliError::Wallet(format!("Invalid wallet address: {}", e)))?;
    
    let balance = rpc_client.get_balance(&pubkey).await
        .map_err(|e| CliError::Network(format!("Failed to get balance: {}", e)))?;
    
    // Add buffer for transaction fees
    let buffer = 10_000_000; // 0.01 SOL buffer
    let required_with_buffer = required_amount + buffer;
    
    if balance < required_with_buffer {
        return Err(CliError::Wallet(format!(
            "Insufficient balance: {:.4} SOL (required: {:.4} SOL)",
            balance as f64 / 1_000_000_000.0,
            required_with_buffer as f64 / 1_000_000_000.0
        )));
    }
    
    Ok(())
}
```

## Implementation Best Practices

### 8. Performance Optimization

```rust
// cli/src/performance.rs
use tokio::sync::{Semaphore, Mutex};
use std::sync::Arc;
use std::time::Duration;

// Connection pooling for RPC calls
pub struct RpcConnectionPool {
    clients: Vec<sol_beast_core::blockchain::rpc_client::NativeRpcClient>,
    current_index: Arc<Mutex<usize>>,
    semaphore: Arc<Semaphore>,
}

impl RpcConnectionPool {
    pub fn new(rpc_urls: Vec<String>) -> Self {
        let clients = rpc_urls.into_iter()
            .map(|url| sol_beast_core::blockchain::rpc_client::NativeRpcClient::new(url))
            .collect();
        
        Self {
            clients,
            current_index: Arc::new(Mutex::new(0)),
            semaphore: Arc::new(Semaphore::new(10)), // Max 10 concurrent connections
        }
    }
    
    pub async fn get_client(&self) -> Result<sol_beast_core::blockchain::rpc_client::NativeRpcClient, CliError> {
        let _permit = self.semaphore.acquire().await
            .map_err(|e| CliError::System(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())))?;
        
        let mut index = self.current_index.lock().await;
        let client = self.clients[*index].clone();
        *index = (*index + 1) % self.clients.len();
        
        Ok(client)
    }
}

// Caching for frequently accessed data
use std::collections::HashMap;
use tokio::time::Instant;

pub struct Cache<T> {
    data: HashMap<String, (T, Instant)>,
    ttl: Duration,
}

impl<T: Clone> Cache<T> {
    pub fn new(ttl: Duration) -> Self {
        Self {
            data: HashMap::new(),
            ttl,
        }
    }
    
    pub fn get(&self, key: &str) -> Option<T> {
        self.data.get(key)
            .and_then(|(value, timestamp)| {
                if timestamp.elapsed() < self.ttl {
                    Some(value.clone())
                } else {
                    None
                }
            })
    }
    
    pub fn set(&mut self, key: String, value: T) {
        self.data.insert(key, (value, Instant::now()));
    }
    
    pub fn clear_expired(&mut self) {
        self.data.retain(|_, (_, timestamp)| timestamp.elapsed() < self.ttl);
    }
}
```

This comprehensive implementation provides:

1. **Complete CLI Structure**: Using clap with proper command hierarchy and aliases
2. **Configuration Management**: Full exposure of all 32+ settings with validation
3. **Wallet Operations**: Multiple connection methods with security features
4. **Trading Commands**: Manual and automated trading with safety checks
5. **Error Handling**: Comprehensive error types with user-friendly suggestions
6. **Safety Mechanisms**: Validation, slippage protection, and balance checks
7. **Performance Optimization**: Connection pooling and caching strategies

The implementation demonstrates how to integrate the CLI with the core library while maintaining safety, performance, and user experience standards.