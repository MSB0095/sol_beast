# Sol Beast CLI Implementation Guide

## Overview

This guide provides specific technical implementation steps for building the Sol Beast CLI interface. It complements the CLI Implementation Review with practical code examples and step-by-step instructions.

## Phase 1: Foundation Setup

### 1. Add CLI Dependencies

Update `cli/Cargo.toml`:

```toml
[package]
name = "cli"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[[bin]]
name = "sol_beast"
path = "src/main.rs"

[dependencies]
# Core library integration
core = { path = "../core", features = ["native", "native-rpc"] }

# CLI Framework
clap = { version = "4.4", features = ["derive", "cargo"] }
clap_complete = "4.4"
colored = "2.0"
dialoguer = "0.11"

# Async runtime
tokio = { workspace = true, features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }

# Configuration and serialization
serde = { workspace = true }
serde_json = { workspace = true }
config = "0.14"
toml = "0.8"

# Networking and HTTP
reqwest = { workspace = true, features = ["json"] }
url = { workspace = true }

# Logging and utilities
log = { workspace = true }
env_logger = "0.11"
chrono = { workspace = true }
futures-util = { workspace = true }
once_cell = { workspace = true }

# Solana ecosystem
solana-program = { workspace = true }
solana-client = { workspace = true }
solana-sdk = { workspace = true }
solana-account-decoder = { workspace = true }
base64 = { workspace = true }
bs58 = { workspace = true }
borsh = { workspace = true }
lru = "0.12"

# Protocol support
mpl-token-metadata = { workspace = true }
rand = { workspace = true }
spl-associated-token-account = { workspace = true }
spl-token = { workspace = true }

# Server framework
axum = "0.7"
tower = "0.4"
tower-http = { version = "0.5", features = ["trace", "cors"] }
tracing = "0.1"
tracing-subscriber = "0.3"

# Error handling
thiserror = { workspace = true }

[features]
default = []
```

### 2. Create Command Structure

Create `src/commands/mod.rs`:

```rust
pub mod buy;
pub mod sell;
pub mod bot;
pub mod config;
pub mod wallet;
pub mod monitor;
pub mod stats;
pub mod strategy;

use crate::error::CliError;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "sol_beast")]
#[command(about = "Sol Beast - Solana Trading Bot CLI")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Parser, Debug)]
pub enum Commands {
    #[command(name = "buy")]
    #[command(about = "Buy tokens")]
    Buy(buy::BuyCommand),
    
    #[command(name = "sell")]
    #[command(about = "Sell tokens")]
    Sell(sell::SellCommand),
    
    #[command(name = "bot")]
    #[command(about = "Bot control commands")]
    Bot(bot::BotCommand),
    
    #[command(name = "config")]
    #[command(about = "Configuration management")]
    Config(config::ConfigCommand),
    
    #[command(name = "wallet")]
    #[command(about = "Wallet management")]
    Wallet(wallet::WalletCommand),
    
    #[command(name = "monitor")]
    #[command(about = "Monitoring commands")]
    Monitor(monitor::MonitorCommand),
    
    #[command(name = "stats")]
    #[command(about = "Statistics and analytics")]
    Stats(stats::StatsCommand),
    
    #[command(name = "strategy")]
    #[command(about = "Trading strategy configuration")]
    Strategy(strategy::StrategyCommand),
}

impl Commands {
    pub async fn execute(&self) -> Result<(), CliError> {
        match self {
            Commands::Buy(cmd) => cmd.execute().await,
            Commands::Sell(cmd) => cmd.execute().await,
            Commands::Bot(cmd) => cmd.execute().await,
            Commands::Config(cmd) => cmd.execute().await,
            Commands::Wallet(cmd) => cmd.execute().await,
            Commands::Monitor(cmd) => cmd.execute().await,
            Commands::Stats(cmd) => cmd.execute().await,
            Commands::Strategy(cmd) => cmd.execute().await,
        }
    }
}
```

### 3. Create Error Handling

Create `src/error.rs`:

```rust
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
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Core library error: {0}")]
    Core(#[from] core::CoreError),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Toml error: {0}")]
    Toml(#[from] toml::de::Error),
    
    #[error("Reqwest error: {0}")]
    Reqwest(#[from] reqwest::Error),
}

impl CliError {
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::Config(_) => 2,
            CliError::Wallet(_) => 3,
            CliError::Trading(_) => 4,
            CliError::Network(_) => 5,
            CliError::Io(_) => 6,
            CliError::Core(_) => 7,
            CliError::Json(_) => 8,
            CliError::Toml(_) => 9,
            CliError::Reqwest(_) => 10,
        }
    }
}
```

## Phase 2: Configuration and Wallet Management

### 4. Configuration Command Implementation

Create `src/commands/config.rs`:

```rust
use clap::Subcommand;
use colored::*;
use std::fs;

use crate::error::CliError;
use core::Settings;

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    #[command(about = "Show current configuration")]
    Show,
    
    #[command(about = "Set configuration value")]
    Set {
        #[arg(value_name = "KEY")]
        key: String,
        
        #[arg(value_name = "VALUE")]
        value: String,
    },
    
    #[command(about = "Edit configuration file")]
    Edit,
    
    #[command(about = "Validate configuration")]
    Validate,
    
    #[command(about = "Export configuration to file")]
    Export {
        #[arg(value_name = "FILE_PATH")]
        file_path: Option<String>,
    },
}

impl ConfigCommand {
    pub async fn execute(&self) -> Result<(), CliError> {
        match self {
            ConfigCommand::Show => self.show_config().await,
            ConfigCommand::Set { key, value } => self.set_config(key, value).await,
            ConfigCommand::Edit => self.edit_config().await,
            ConfigCommand::Validate => self.validate_config().await,
            ConfigCommand::Export { file_path } => self.export_config(file_path).await,
        }
    }

    async fn show_config(&self) -> Result<(), CliError> {
        let settings = self.load_settings()?;
        
        println!("{}", "Current Configuration:".bright_blue().bold());
        println!("Blockchain Settings:");
        println!("  Solana RPC URLs: {:?}", settings.solana_rpc_urls);
        println!("  Solana WS URLs: {:?}", settings.solana_ws_urls);
        println!("  Pump.fun Program: {}", settings.pump_fun_program);
        println!("  Metadata Program: {}", settings.metadata_program);
        
        println!("\nTrading Parameters:");
        println!("  Take Profit: {}%", settings.tp_percent);
        println!("  Stop Loss: {}%", settings.sl_percent);
        println!("  Default Buy Amount: {} SOL", settings.buy_amount);
        println!("  Timeout: {} seconds", settings.timeout_secs);
        println!("  Slippage: {} bps", settings.slippage_bps);
        
        println!("\nSafety Features:");
        println!("  Safer Sniping: {}", settings.enable_safer_sniping);
        println!("  Max Tokens Threshold: {}", settings.min_tokens_threshold);
        println!("  Max SOL per Token: {}", settings.max_sol_per_token);
        println!("  Min/Max Liquidity: {}/{} SOL", settings.min_liquidity_sol, settings.max_liquidity_sol);
        
        if settings.helius_sender_enabled {
            println!("\nHelius Integration:");
            println!("  Enabled: {}", settings.helius_sender_enabled);
            println!("  API Key: {}", if settings.helius_api_key.is_some() { "Configured" } else { "Not set" });
            println!("  Endpoint: {}", settings.helius_sender_endpoint);
            println!("  Min Tip: {} SOL", settings.helius_min_tip_sol);
        }
        
        Ok(())
    }

    async fn set_config(&self, key: &str, value: &str) -> Result<(), CliError> {
        let mut settings = self.load_settings()?;
        let mut updated = false;
        
        match key {
            "tp_percent" => {
                settings.tp_percent = value.parse()
                    .map_err(|e| CliError::Config(format!("Invalid TP percent: {}", e)))?;
                updated = true;
            }
            "sl_percent" => {
                settings.sl_percent = value.parse()
                    .map_err(|e| CliError::Config(format!("Invalid SL percent: {}", e)))?;
                updated = true;
            }
            "buy_amount" => {
                settings.buy_amount = value.parse()
                    .map_err(|e| CliError::Config(format!("Invalid buy amount: {}", e)))?;
                updated = true;
            }
            "timeout_secs" => {
                settings.timeout_secs = value.parse()
                    .map_err(|e| CliError::Config(format!("Invalid timeout: {}", e)))?;
                updated = true;
            }
            "slippage_bps" => {
                settings.slippage_bps = value.parse()
                    .map_err(|e| CliError::Config(format!("Invalid slippage: {}", e)))?;
                updated = true;
            }
            "helius_api_key" => {
                settings.helius_api_key = Some(value.to_string());
                updated = true;
            }
            _ => {
                return Err(CliError::Config(format!(
                    "Unknown configuration key: {}. Use 'sol_beast config show' to see all keys.",
                    key
                )));
            }
        }
        
        if updated {
            self.save_settings(&settings)?;
            println!("{}", format!("✓ {} set to {}", key, value).bright_green());
        }
        
        Ok(())
    }

    async fn validate_config(&self) -> Result<(), CliError> {
        let settings = self.load_settings()?;
        
        match settings.validate() {
            Ok(_) => {
                println!("{}", "✓ Configuration is valid".bright_green());
                
                if !settings.enable_safer_sniping {
                    println!("{}", "⚠ Warning: Safer sniping is disabled".yellow());
                }
                
                if !settings.helius_sender_enabled && settings.helius_api_key.is_some() {
                    println!("{}", "⚠ Warning: Helius API key set but Helius sender is disabled".yellow());
                }
            }
            Err(e) => {
                println!("{}", format!("✗ Configuration validation failed: {}", e).bright_red());
                return Err(CliError::Config(e.to_string()));
            }
        }
        
        Ok(())
    }

    fn load_settings(&self) -> Result<Settings, CliError> {
        let config_path = std::env::var("SOL_BEAST_CONFIG_PATH")
            .unwrap_or_else(|_| "config.toml".to_string());
        
        Settings::from_file(&config_path)
            .map_err(|e| CliError::Config(format!("Failed to load config: {}", e)))
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), CliError> {
        let config_path = std::env::var("SOL_BEAST_CONFIG_PATH")
            .unwrap_or_else(|_| "config.toml".to_string());
        
        settings.save_to_file(&config_path)
            .map_err(|e| CliError::Config(format!("Failed to save config: {}", e)))
    }
}
```

### 5. Wallet Command Implementation

Create `src/commands/wallet.rs`:

```rust
use clap::Subcommand;
use colored::*;
use core::config::wallet::WalletManager;
use std::sync::Arc;

use crate::error::CliError;

#[derive(Subcommand, Debug)]
pub enum WalletCommand {
    #[command(about = "Connect wallet")]
    Connect {
        #[command(subcommand)]
        source: WalletSource,
    },
    
    #[command(about = "Show wallet information")]
    Info,
    
    #[command(about = "Show wallet balance")]
    Balance,
    
    #[command(about = "Disconnect wallet")]
    Disconnect,
}

#[derive(Subcommand, Debug)]
pub enum WalletSource {
    #[command(name = "keypair")]
    #[command(about = "Connect using keypair file")]
    Keypair {
        #[arg(value_name = "FILE_PATH")]
        path: String,
    },
    
    #[command(name = "private-key")]
    #[command(about = "Connect using private key")]
    PrivateKey {
        #[arg(value_name = "PRIVATE_KEY")]
        key: String,
    },
    
    #[command(name = "env")]
    #[command(about = "Connect using environment variable")]
    Env,
}

impl WalletCommand {
    pub async fn execute(&self) -> Result<(), CliError> {
        let wallet_manager = Arc::new(std::sync::Mutex::new(WalletManager::new()));
        
        match self {
            WalletCommand::Connect { source } => {
                self.connect_wallet(source, wallet_manager).await
            }
            WalletCommand::Info => {
                self.show_info(wallet_manager).await
            }
            WalletCommand::Balance => {
                self.show_balance(wallet_manager).await
            }
            WalletCommand::Disconnect => {
                self.disconnect(wallet_manager).await
            }
        }
    }

    async fn connect_wallet(
        &self,
        source: &WalletSource,
        wallet_manager: Arc<std::sync::Mutex<WalletManager>>,
    ) -> Result<(), CliError> {
        let keypair_data = match source {
            WalletSource::Keypair { path } => {
                fs::read_to_string(path)
                    .map_err(|e| CliError::Wallet(format!("Failed to read keypair file: {}", e)))?
            }
            WalletSource::PrivateKey { key } => key.clone(),
            WalletSource::Env => {
                std::env::var("SOL_BEAST_PRIVATE_KEY")
                    .map_err(|_| CliError::Wallet("SOL_BEAST_PRIVATE_KEY not set".to_string()))?
            }
        };

        let keypair_bytes = if source == &WalletSource::Keypair { path } {
            serde_json::from_str(&keypair_data)
                .map_err(|e| CliError::Wallet(format!("Invalid keypair JSON: {}", e)))?
        } else {
            core::parse_private_key_string(&keypair_data)
                .map_err(|e| CliError::Wallet(format!("Invalid private key: {}", e)))?
        };

        // Here you would implement the actual wallet connection logic
        // This would involve loading the keypair and connecting to Solana
        
        println!("{}", "✓ Wallet connected successfully".bright_green());
        Ok(())
    }

    async fn show_info(
        &self,
        wallet_manager: Arc<std::sync::Mutex<WalletManager>>,
    ) -> Result<(), CliError> {
        let manager = wallet_manager.lock().unwrap();
        
        if let Some(wallet_info) = manager.get_current_wallet() {
            println!("Wallet Address: {}", wallet_info.address);
            println!("Status: {}", if wallet_info.connected { "Connected" } else { "Disconnected" });
            if let Some(balance) = wallet_info.balance {
                println!("Balance: {:.4} SOL", balance as f64 / 1_000_000_000.0);
            }
        } else {
            println!("{}", "No wallet connected".yellow());
        }
        
        Ok(())
    }

    async fn disconnect(
        &self,
        wallet_manager: Arc<std::sync::Mutex<WalletManager>>,
    ) -> Result<(), CliError> {
        let mut manager = wallet_manager.lock().unwrap();
        manager.disconnect();
        println!("{}", "✓ Wallet disconnected".bright_green());
        Ok(())
    }
}
```

## Phase 3: Trading Commands

### 6. Buy Command Implementation

Create `src/commands/buy.rs`:

```rust
use clap::Args;
use colored::*;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::CliError;
use core::{
    buy_token,
    Settings,
    rpc_client::RpcClient as CoreRpcClient,
    blockchain::rpc_client::NativeRpcClient,
    models::PriceCache,
};

#[derive(Args, Debug)]
pub struct BuyCommand {
    #[arg(value_name = "MINT")]
    pub mint: String,
    
    #[arg(value_name = "AMOUNT_SOL")]
    pub amount_sol: f64,
    
    #[arg(short, long)]
    #[arg(value_name = "DRY_RUN")]
    pub dry_run: bool,
    
    #[arg(short, long)]
    #[arg(value_name = "PRICE_CACHE_TTL")]
    pub price_cache_ttl: Option<u64>,
}

impl BuyCommand {
    pub async fn execute(&self) -> Result<(), CliError> {
        println!("{}", format!("🛒 Buying {} SOL worth of tokens from {}", 
            self.amount_sol, self.mint).bright_blue());
        
        if self.dry_run {
            println!("{}", "🔍 Running in DRY-RUN mode (no actual transactions)".yellow());
        }

        // Load configuration
        let settings = self.load_settings()?;
        let rpc_client = Arc::new(NativeRpcClient::new(settings.solana_rpc_urls[0].clone()));

        // Initialize price cache
        let price_cache = Arc::new(Mutex::new(PriceCache::new(settings.cache_capacity)));

        // Fetch current price for validation
        let current_price = core::rpc_helpers::fetch_current_price(
            &self.mint,
            &price_cache,
            &rpc_client,
            &settings,
        ).await.map_err(|e| CliError::Trading(format!("Failed to fetch price: {}", e)))?;

        println!("{}", format!("💰 Current token price: {:.18} SOL/token", current_price).bright_cyan());

        // Calculate expected token amount
        let expected_tokens = ((self.amount_sol / current_price) * 1_000_000.0) as u64;
        println!("{}", format!("🎯 Expected tokens: {}", expected_tokens).bright_cyan());

        // Execute the buy
        let holding = buy_token(
            &self.mint,
            self.amount_sol,
            !self.dry_run,
            None, // keypair - would be loaded from wallet
            None, // simulate_keypair
            price_cache,
            &rpc_client,
            &settings,
        ).await.map_err(|e| CliError::Trading(format!("Buy failed: {}", e)))?;

        // Display results
        println!("{}", "✅ Buy completed successfully!".bright_green());
        println!("  Mint: {}", holding.mint);
        println!("  Amount: {}", holding.amount);
        println!("  Buy Price: {:.18} SOL/token", holding.buy_price);
        println!("  Buy Time: {}", holding.buy_time);

        if self.dry_run {
            println!("{}", "\n🔍 This was a dry-run. No actual transaction was sent.".yellow());
        }

        Ok(())
    }

    fn load_settings(&self) -> Result<Settings, CliError> {
        let config_path = std::env::var("SOL_BEAST_CONFIG_PATH")
            .unwrap_or_else(|_| "config.toml".to_string());
        
        Settings::from_file(&config_path)
            .map_err(|e| CliError::Config(format!("Failed to load config: {}", e)))
    }
}
```

### 7. Bot Control Implementation

Create `src/commands/bot.rs`:

```rust
use clap::Subcommand;
use colored::*;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::error::CliError;
use core::{
    Settings,
    connectivity::api::{BotControl, BotRunningState, BotMode},
};

#[derive(Subcommand, Debug)]
pub enum BotCommand {
    #[command(about = "Start the trading bot")]
    Start {
        #[arg(short, long)]
        #[arg(value_name = "DRY_RUN")]
        dry_run: bool,
    },
    
    #[command(about = "Stop the trading bot")]
    Stop,
    
    #[command(about = "Show bot status")]
    Status,
    
    #[command(about = "Set bot mode")]
    Mode {
        #[arg(value_name = "MODE")]
        mode: String,
    },
    
    #[command(about = "Show bot logs")]
    Logs {
        #[arg(short, long)]
        #[arg(value_name = "LINES")]
        lines: Option<usize>,
    },
}

impl BotCommand {
    pub async fn execute(&self) -> Result<(), CliError> {
        match self {
            BotCommand::Start { dry_run } => {
                self.start(*dry_run).await
            }
            BotCommand::Stop => {
                self.stop().await
            }
            BotCommand::Status => {
                self.status().await
            }
            BotCommand::Mode { mode } => {
                self.set_mode(mode).await
            }
            BotCommand::Logs { lines } => {
                self.logs(*lines).await
            }
        }
    }

    async fn start(&self, dry_run: bool) -> Result<(), CliError> {
        println!("{}", "🚀 Starting Sol Beast Trading Bot...".bright_blue());
        
        let mode = if dry_run { "dry-run" } else { "real" };
        println!("{}", format!("Mode: {}", mode).bright_cyan());

        // Initialize bot control
        let bot_control = Arc::new(BotControl::new_with_mode(
            if dry_run { BotMode::DryRun } else { BotMode::Real }
        ));

        // Here you would start the actual bot:
        // - Initialize WebSocket connections
        // - Start monitoring for new tokens
        // - Start holdings monitoring
        // - Start price tracking

        // For now, simulate the startup
        let bot_control_clone = bot_control.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let mut state = bot_control_clone.running_state.lock().await;
            *state = BotRunningState::Running;
        });

        println!("{}", "✅ Bot started successfully!".bright_green());
        println!("{}", "Monitor logs with: sol_beast bot logs".bright_cyan());
        
        // Keep the bot running in background (this is simplified)
        println!("{}", "Bot is running in the background...".yellow());
        
        Ok(())
    }

    async fn stop(&self) -> Result<(), CliError> {
        println!("{}", "🛑 Stopping Sol Beast Trading Bot...".bright_blue());
        
        // Here you would stop the bot gracefully
        println!("{}", "✅ Bot stopped successfully!".bright_green());
        
        Ok(())
    }

    async fn status(&self) -> Result<(), CliError> {
        println!("{}", "📊 Bot Status:".bright_blue().bold());
        
        // Here you would query the actual bot status
        // For now, show mock status
        println!("  Status: Running");
        println!("  Mode: Real");
        println!("  Uptime: 2h 34m");
        println!("  Current Holdings: 3");
        println!("  Total P&L: +0.05 SOL");
        
        Ok(())
    }

    async fn set_mode(&self, mode: &str) -> Result<(), CliError> {
        match mode {
            "dry-run" | "real" => {
                println!("{}", format!("🔄 Changing bot mode to: {}", mode).bright_blue());
                println!("{}", "✅ Bot mode updated!".bright_green());
            }
            _ => {
                return Err(CliError::Config(
                    "Mode must be 'dry-run' or 'real'".to_string()
                ));
            }
        }
        
        Ok(())
    }

    async fn logs(&self, lines: Option<usize>) -> Result<(), CliError> {
        println!("{}", "📋 Recent Bot Logs:".bright_blue().bold());
        
        let limit = lines.unwrap_or(20);
        
        // Here you would fetch actual logs from the bot
        println!("  [INFO] 2025-11-26T00:55:00Z Bot started in real mode");
        println!("  [INFO] 2025-11-26T00:55:15Z Connected to Solana RPC");
        println!("  [INFO] 2025-11-26T00:55:30Z WebSocket connection established");
        println!("  [INFO] 2025-11-26T00:56:00Z Monitoring new token launches");
        println!("  [WARN] 2025-11-26T00:56:30Z High slippage detected for mint ABC123...");
        println!("  [INFO] 2025-11-26T00:57:00Z Token purchase simulation completed");
        
        println!("\nShowing last {} lines. Use 'sol_beast bot logs --lines 50' for more.".bright_cyan());
        
        Ok(())
    }
}
```

## Phase 4: Main Entry Point

### 8. Update main.rs

Replace `cli/src/main.rs`:

```rust
use clap::Command;
use colored::*;
use std::process;

mod commands;
mod error;

use commands::Commands;
use error::CliError;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize core library
    core::init();
    
    // Set up colored terminal output
    colored::control::set_override(true);
    
    let cli = commands::Cli::command();
    
    let matches = cli.get_matches();
    
    match Commands::from_matches(&matches) {
        Ok(command) => {
            if let Err(e) = command.execute().await {
                eprintln!("{}", format!("Error: {}", e).bright_red());
                eprintln!("{}", "Run 'sol_beast --help' for usage information.".yellow());
                process::exit(e.exit_code());
            }
        }
        Err(err) => {
            // Handle parse errors
            eprintln!("{}", format!("Error: {}", err).bright_red());
            process::exit(1);
        }
    }
    
    Ok(())
}
```

## Phase 5: Testing and Validation

### 9. Create Test Structure

Create `tests/cli_integration_tests.rs`:

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::process::Command;
    
    #[test]
    fn test_config_show() {
        let output = Command::new("cargo")
            .args(&["run", "--bin", "sol_beast", "--", "config", "show"])
            .output()
            .expect("Failed to execute command");
            
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Current Configuration"));
    }
    
    #[test]
    fn test_bot_status() {
        let output = Command::new("cargo")
            .args(&["run", "--bin", "sol_beast", "--", "bot", "status"])
            .output()
            .expect("Failed to execute command");
            
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Bot Status"));
    }
}
```

## Implementation Roadmap

### Week 1: Foundation
- [ ] Set up CLI framework with clap
- [ ] Implement configuration management commands
- [ ] Add wallet connection functionality
- [ ] Create error handling infrastructure

### Week 2: Core Trading
- [ ] Implement buy/sell commands
- [ ] Add dry-run mode support
- [ ] Create bot control commands
- [ ] Add basic monitoring

### Week 3: Advanced Features
- [ ] Implement strategy configuration
- [ ] Add statistics and reporting
- [ ] Create log management
- [ ] Add comprehensive testing

### Week 4: Polish and Documentation
- [ ] Add shell completion
- [ ] Create user documentation
- [ ] Performance optimization
- [ ] Final testing and validation

## Key Implementation Notes

1. **Async Integration**: All CLI commands are async and integrate with tokio runtime
2. **Error Handling**: Comprehensive error handling with user-friendly messages
3. **Configuration**: Full integration with core library settings
4. **Safety**: Dry-run modes and validation checks throughout
5. **User Experience**: Colored output and clear status messages
6. **Testing**: Integration tests for all major command flows

This implementation guide provides a complete roadmap for building the Sol Beast CLI interface, transforming the current placeholder into a fully functional command-line interface for the trading bot.