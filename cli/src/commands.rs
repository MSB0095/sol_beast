use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "sol-beast")]
#[command(about = "Advanced Solana trading bot CLI interface")]
#[command(version = "0.1.0")]
pub struct Cli {
    /// Configuration file path
    #[arg(short, long, default_value = "~/.config/sol_beast/config.toml")]
    pub config: PathBuf,
    
    /// Enable verbose output
    #[arg(short, long)]
    pub verbose: bool,
    
    /// Simulate operations without making changes
    #[arg(short, long)]
    pub dry_run: bool,
    
    /// Command to execute
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum OutputFormat {
    Json,
    Table,
    Plain,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq)]
pub enum BotMode {
    Live,
    DryRun,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Configuration management commands
    Config { #[command(subcommand)] command: ConfigCommands },
    
    /// Wallet operations
    Wallet { #[command(subcommand)] command: WalletCommands },
    
    /// Trading operations
    Trade { #[command(subcommand)] command: TradeCommands },
    
    /// Bot control and management
    Bot { #[command(subcommand)] command: BotCommands },
    
    // Common aliases removed to avoid duplicate command names (use `bot start/stop/status`)
    
    /// Generate shell completion scripts
    Completion {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

/// Configuration management commands
#[derive(Subcommand, Debug, Clone)]
pub enum ConfigCommands {
    /// Get configuration value
    Get { key: String },
    
    /// Set configuration value
    Set { key: String, value: String },
    
    /// Show configuration
    Show {
        #[arg(short, long)]
        format: Option<OutputFormat>,
        #[arg(short, long)]
        sensitive: bool,
    },
    
    /// Validate configuration
    Validate {
        #[arg(short, long)]
        file: Option<PathBuf>,
    },
    
    /// Configuration templates
    Templates {
        #[command(subcommand)]
        command: TemplateCommands,
    },
}

/// Configuration template commands
#[derive(Subcommand, Debug, Clone)]
pub enum TemplateCommands {
    /// List available templates
    List,
    
    /// Generate configuration from template
    Generate {
        template: String,
        #[arg(short, long, default_value = "config.toml")]
        output: PathBuf,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum WalletCommands {
    /// Connect wallet
    Connect {
        #[arg(short, long)]
        keypair: Option<PathBuf>,
    },
    /// Show wallet status
    Status,
    /// Show wallet balance
    Balance,
}

#[derive(Subcommand, Debug, Clone)]
pub enum TradeCommands {
    /// Buy tokens
    Buy { mint: String, #[arg(short, long)] amount: Option<f64> },
    /// Sell tokens
    Sell { mint: String, #[arg(short, long)] percentage: Option<f64> },
    /// Show portfolio
    Portfolio {
        #[arg(short, long)]
        format: Option<OutputFormat>,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum BotCommands {
    /// Start the trading bot
    Start {
        #[arg(short, long)]
        mode: Option<BotMode>,
    },
    /// Stop the trading bot
    Stop,
    /// Bot status
    Status {
        #[arg(short, long)]
        detailed: bool,
    },
}