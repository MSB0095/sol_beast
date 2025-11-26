mod commands;

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;
use std::path::PathBuf;
use commands::Cli;
use core_lib as core;
use std::sync::Arc;
use std::num::NonZeroUsize;
use tokio::sync::Mutex;
use core::core::models::PriceCache;
use core::blockchain::rpc_client::native::NativeRpcClient;
use core::blockchain::signer::native::NativeKeypairSigner;
use solana_sdk::signature::Keypair;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    
    // Initialize logging
    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }
    
    // Handle command execution
    if let Err(error) = handle_command(cli).await {
        eprintln!("{} {}", "ERROR:".red(), error);
        std::process::exit(1);
    }
    
    Ok(())
}

async fn handle_command(cli: Cli) -> Result<(), String> {
    match &cli.command {
        commands::Commands::Config { command: config_cmd } => {
            handle_config_command(config_cmd, &cli).await
        }
        
        commands::Commands::Wallet { command: _ } => {
            println!("Wallet commands coming soon!");
            Ok(())
        }
        
        commands::Commands::Trade { command: trade_cmd } => {
            match trade_cmd {
                commands::TradeCommands::Buy { mint, amount } => {
                    // Load settings and prepare args for core::trading::buy_token
                    let settings = load_config(&cli).await.map_err(|e| e.to_string())?;
                    let amount = amount.unwrap_or(settings.buy_amount);
                    let is_real = !cli.dry_run;
                    // Prepare RPC client and price cache
                    let rpc_client: Arc<dyn core::blockchain::rpc_client::RpcClient> = Arc::new(NativeRpcClient::new(settings.solana_rpc_urls[0].clone()));
                    let capacity = NonZeroUsize::new(settings.cache_capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
                    let price_cache = Arc::new(Mutex::new(PriceCache::new(capacity)));
                    // Prepare signer (if real) and simulator signer
                    let payer: Option<Arc<dyn core::blockchain::signer::Signer>> = if is_real {
                        // Try to use wallet private key if provided
                        if let Some(pk_str) = &settings.wallet_private_key_string {
                            let bytes = core::config::settings::parse_private_key_string(pk_str).map_err(|e| e.to_string())?;
                            let keypair = Keypair::try_from(bytes.as_slice()).map_err(|e| e.to_string())?;
                            Some(Arc::new(NativeKeypairSigner::new(Arc::new(keypair))))
                        } else {
                            return Err("No wallet private key configured for real buys".to_string());
                        }
                    } else {
                        None
                    };
                    let sim_signer: Option<Arc<dyn core::blockchain::signer::Signer>> = Some(Arc::new(NativeKeypairSigner::new(Arc::new(Keypair::new()))));

                    match core::trading::buyer::buy_token(
                        mint,
                        amount,
                        is_real,
                        payer,
                        sim_signer,
                        price_cache,
                        &rpc_client,
                        &Arc::new(settings),
                    ).await {
                        Ok(holding) => {
                            println!("Buy executed: mint={}, amount={}, buy_price={} SOL", holding.mint, holding.amount, holding.buy_price);
                            Ok(())
                        }
                        Err(e) => Err(e.to_string()),
                    }
                }
                _ => {
                    println!("Trading commands coming soon!");
                    Ok(())
                }
            }
        }
        
        commands::Commands::Bot { command: _ } => {
            println!("Bot commands coming soon!");
            Ok(())
        }
        
        // Aliases removed; use `bot start|stop|status` subcommands instead
        
        commands::Commands::Completion { shell, output } => {
            handle_completion_generation(shell.clone(), output.clone())
        }
    }
}

async fn handle_config_command(command: &commands::ConfigCommands, cli: &Cli) -> Result<(), String> {
    match command {
        commands::ConfigCommands::Get { key } => {
            handle_config_get(key, cli).await
        }
        commands::ConfigCommands::Set { key, value } => {
            handle_config_set(key, value, cli).await
        }
        commands::ConfigCommands::Show { format, sensitive } => {
            handle_config_show(format, sensitive, cli).await
        }
        commands::ConfigCommands::Validate { file } => {
            handle_config_validate(file, cli).await
        }
        commands::ConfigCommands::Templates { command } => {
            handle_template_command(command, cli).await
        }
    }
}

async fn handle_config_get(key: &str, cli: &Cli) -> Result<(), String> {
    // Delegate to core crate for actual implementation
    let settings = load_config(cli).await
        .map_err(|e| e.to_string())?;
    
    // Use core crate's configuration logic
    let value = get_config_value(&settings, key)
        .ok_or_else(|| format!("Setting '{}' not found", key))?;
    
    println!("{}", value);
    Ok(())
}

async fn handle_config_set(key: &str, value: &str, cli: &Cli) -> Result<(), String> {
    if cli.dry_run {
        println!("DRY RUN: Configuration changes not applied");
        return Ok(());
    }
    
    let mut settings = load_config(cli).await
        .map_err(|e| e.to_string())?;
    
    // Delegate validation and setting to core crate
    set_config_value(&mut settings, key, value)
        .map_err(|e| e.to_string())?;
    
    // Save using core crate's save functionality
    settings.save_to_file(&cli.config.to_string_lossy())
        .map_err(|e| e.to_string())?;
    
    println!("Configuration '{}' updated successfully", key);
    Ok(())
}

async fn handle_config_show(format: &Option<commands::OutputFormat>, sensitive: &bool, cli: &Cli) -> Result<(), String> {
    let settings = load_config(cli).await
        .map_err(|e| e.to_string())?;
    
    match format {
        Some(commands::OutputFormat::Json) => {
            let json = serde_json::to_string_pretty(&settings)
                .map_err(|e| e.to_string())?;
            println!("{}", json);
        }
        _ => {
            print_config_table(&settings, *sensitive);
        }
    }
    
    Ok(())
}

async fn handle_config_validate(file: &Option<PathBuf>, cli: &Cli) -> Result<(), String> {
    let config_path = file.as_ref().unwrap_or(&cli.config);
    
    if !config_path.exists() {
        return Err(format!("Configuration file not found: {}", config_path.display()));
    }
    
    let settings = core::Settings::from_file(&config_path.to_string_lossy())
        .map_err(|e| e.to_string())?;
    
    // Use core crate's validation
    settings.validate()
        .map_err(|e| e.to_string())?;
    
    println!("Configuration validation passed");
    Ok(())
}

async fn handle_template_command(command: &commands::TemplateCommands, cli: &Cli) -> Result<(), String> {
    match command {
        commands::TemplateCommands::List => {
            let templates = vec![
                "beginner - Safe settings for new users",
                "conservative - Low-risk trading parameters", 
                "aggressive - High-performance settings",
            ];
            
            println!("Available configuration templates:");
            for template in templates {
                println!("  {}", template);
            }
        }
        commands::TemplateCommands::Generate { template, output } => {
            generate_config_template(template, output, cli).await?;
        }
    }
    
    Ok(())
}

fn handle_completion_generation(shell: clap_complete::Shell, output: Option<PathBuf>) -> Result<(), String> {
    let mut cmd = Cli::command();
    
    let mut buf = Vec::new();
    generate(shell, &mut cmd, "sol-beast", &mut buf);
    
    match output {
        Some(output_path) => {
            std::fs::write(&output_path, buf)
                .map_err(|e| e.to_string())?;
            println!("Completions written to: {}", output_path.display());
        }
        None => {
            print!("{}", String::from_utf8_lossy(&buf));
        }
    }
    
    Ok(())
}

// Minimal utility functions that delegate to core crate
async fn load_config(cli: &Cli) -> Result<core::Settings, Box<dyn std::error::Error>> {
    let config_path = if cli.config.starts_with("~") {
        let expanded = cli.config.strip_prefix("~").unwrap();
        let home = std::env::var("HOME")
            .map_err(|_| core::core::error::CoreError::Storage("HOME not found".to_string()))?;
        PathBuf::from(format!("{}{}", home, expanded.to_string_lossy()))
    } else {
        cli.config.clone()
    };
    
    if config_path.exists() {
        let settings = core::Settings::from_file(&config_path.to_string_lossy())?;
        Ok(settings)
    } else {
        // Fallback to example config
        let example = std::fs::read_to_string("config.example.toml")?;
        let settings = core::Settings::from_toml_str(&example)?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        settings.save_to_file(&config_path.to_string_lossy())?;
        Ok(settings)
    }
}

fn get_config_value(settings: &core::Settings, path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('.').collect();
    
    match parts.as_slice() {
        ["trading", "tp_percent"] => Some(settings.tp_percent.to_string()),
        ["trading", "sl_percent"] => Some(settings.sl_percent.to_string()),
        ["trading", "buy_amount"] => Some(settings.buy_amount.to_string()),
        ["network", "rpc_urls"] => Some(settings.solana_rpc_urls.join(", ")),
        ["safety", "slippage_bps"] => Some(settings.slippage_bps.to_string()),
        _ => None,
    }
}

fn set_config_value(settings: &mut core::Settings, path: &str, value: &str) -> Result<(), core::core::error::CoreError> {
    let parts: Vec<&str> = path.split('.').collect();
    
    match parts.as_slice() {
        ["trading", "tp_percent"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| core::core::error::CoreError::Serialization("Invalid tp_percent".to_string()))?;
            settings.tp_percent = parsed;
        }
        ["trading", "sl_percent"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| core::core::error::CoreError::Serialization("Invalid sl_percent".to_string()))?;
            settings.sl_percent = parsed;
        }
        ["trading", "buy_amount"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| core::core::error::CoreError::Serialization("Invalid buy_amount".to_string()))?;
            settings.buy_amount = parsed;
        }
        ["safety", "slippage_bps"] => {
            let parsed: u64 = value.parse()
                .map_err(|_| core::core::error::CoreError::Serialization("Invalid slippage_bps".to_string()))?;
            settings.slippage_bps = parsed;
        }
        _ => return Err(core::core::error::CoreError::Serialization(format!("Unknown config path: {}", path))),
    }
    
    Ok(())
}

fn print_config_table(settings: &core::Settings, sensitive: bool) {
    println!("Sol Beast Configuration");
    println!("{:<30} {:<20}", "Setting", "Value");
    println!("{}", "-".repeat(50));
    
    let configs = vec![
        ("tp_percent", settings.tp_percent.to_string()),
        ("sl_percent", settings.sl_percent.to_string()),
        ("buy_amount", settings.buy_amount.to_string()),
        ("slippage_bps", settings.slippage_bps.to_string()),
    ];
    
    for (key, value) in configs {
        let display_value = if sensitive || key.contains("key") {
            "***redacted***".to_string()
        } else {
            value
        };
        println!("{:<30} {:<20}", key, display_value);
    }
}

async fn generate_config_template(template: &str, output: &PathBuf, cli: &Cli) -> Result<(), String> {
    if cli.dry_run {
        println!("DRY RUN: Would generate template '{}' to '{}'", template, output.display());
        return Ok(());
    }
    
    let template_content = match template {
        "beginner" => generate_beginner_template(),
        "conservative" => generate_conservative_template(),
        "aggressive" => generate_aggressive_template(),
        _ => return Err(format!("Unknown template: {}", template)),
    };
    
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| e.to_string())?;
    }
    
    std::fs::write(output, template_content)
        .map_err(|e| e.to_string())?;
    
    println!("Generated '{}' configuration template: {}", template, output.display());
    Ok(())
}

fn generate_beginner_template() -> String {
    r#"# Beginner Configuration Template for Sol Beast
tp_percent = 30.0
sl_percent = 10.0
buy_amount = 0.01
slippage_bps = 200
"#.to_string()
}

fn generate_conservative_template() -> String {
    r#"# Conservative Configuration Template for Sol Beast
tp_percent = 20.0
sl_percent = 8.0
buy_amount = 0.005
slippage_bps = 100
"#.to_string()
}

fn generate_aggressive_template() -> String {
    r#"# Aggressive Configuration Template for Sol Beast
tp_percent = 100.0
sl_percent = 5.0
buy_amount = 0.05
slippage_bps = 300
"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn verify_cli() {
        Cli::command().try_get_matches_from(vec!["sol-beast", "--help"]).ok();
    }
    
    #[test]
    fn verify_config_commands() {
        Cli::try_parse_from(vec!["sol-beast", "config", "get", "tp_percent"]).ok();
        Cli::try_parse_from(vec!["sol-beast", "config", "set", "tp_percent", "50.0"]).ok();
        Cli::try_parse_from(vec!["sol-beast", "config", "show"]).ok();
    }
}
