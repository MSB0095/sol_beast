mod commands;

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;
use std::path::PathBuf;
use commands::Cli;
use core_lib as core;
use once_cell::sync::OnceCell;
use tokio::sync::RwLock as TokioRwLock;
use std::sync::Arc;
use std::num::NonZeroUsize;
use tokio::sync::Mutex;
use core::models::PriceCache;
use core::blockchain::rpc_client::native::NativeRpcClient;
use core::blockchain::signer::native::NativeKeypairSigner;
use solana_sdk::signature::Keypair;
use log::{info};
#[cfg(unix)]
use tokio::signal::unix::{signal, SignalKind};

// Global runtime handle for in-process server/monitor
static RUNTIME_HANDLE: OnceCell<Arc<TokioRwLock<Option<(core::connectivity::api::ServerHandle, core::trading::monitor::MonitorHandle)>>>> = OnceCell::new();

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
    handle_command_with_client(cli, None).await
}

async fn handle_command_with_client(cli: Cli, rpc_client_override: Option<Arc<dyn core::blockchain::rpc_client::RpcClient>>) -> Result<(), String> {
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
                    println!("Loading config from: {}", cli.config.display());
                    let settings = load_config(&cli).await.map_err(|e| e.to_string())?;
                    let amount = amount.unwrap_or(settings.buy_amount);
                    let is_real = !cli.dry_run;
                    // Prepare RPC client and price cache
                    let rpc_client: Arc<dyn core::blockchain::rpc_client::RpcClient> = if let Some(override_client) = rpc_client_override.clone() { override_client } else { Arc::new(NativeRpcClient::new(settings.solana_rpc_urls[0].clone())) };
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
        
        commands::Commands::Bot { command } => {
            match command {
                commands::BotCommands::Start { mode } => {
                    // start server mode
                    let settings = load_config(&cli).await.map_err(|e| e.to_string())?;
                    let bind_addr = std::env::var("API_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
                    println!("Using API_BIND_ADDR: {}", bind_addr);
                    // Build ApiState
                    let runtime_mode = match mode {
                        Some(commands::BotMode::Live) => core::connectivity::api::BotMode::Real,
                        _ => if cli.dry_run { core::connectivity::api::BotMode::DryRun } else { core::connectivity::api::BotMode::Real },
                    };

                    let api_state = core::connectivity::api::ApiState {
                        settings: Arc::new(Mutex::new(settings.clone())),
                        stats: Arc::new(Mutex::new(core::connectivity::api::BotStats {
                            total_buys: 0,
                            total_sells: 0,
                            total_profit: 0.0,
                            current_holdings: Vec::new(),
                            uptime_secs: 0,
                            last_activity: chrono::Utc::now().to_rfc3339(),
                            running_state: None,
                            mode: None,
                            runtime_mode: None,
                        })),
                        bot_control: Arc::new(core::connectivity::api::BotControl::new_with_mode(runtime_mode)),
                        detected_coins: Arc::new(Mutex::new(Vec::new())),
                        trades: Arc::new(Mutex::new(Vec::new())),
                    };

                    // Build RPC client and signer
                    let rpc_client: Arc<dyn core::blockchain::rpc_client::RpcClient> = if let Some(override_client) = rpc_client_override.clone() { override_client } else { Arc::new(core::blockchain::rpc_client::native::NativeRpcClient::new(settings.solana_rpc_urls[0].clone())) };

                    let simulate_keypair: Option<Arc<dyn core::blockchain::signer::Signer>> = Some(Arc::new(core::blockchain::signer::native::NativeKeypairSigner::new(Arc::new(Keypair::new()))));
                    let keypair: Option<Arc<dyn core::blockchain::signer::Signer>> = if !cli.dry_run {
                        if let Some(pk_str) = &settings.wallet_private_key_string {
                            let bytes = core::config::settings::parse_private_key_string(pk_str).map_err(|e| e.to_string())?;
                            let keypair = Keypair::try_from(bytes.as_slice()).map_err(|e| e.to_string())?;
                            Some(Arc::new(core::blockchain::signer::native::NativeKeypairSigner::new(Arc::new(keypair))))
                        } else { None }
                    } else { None };

                    // Price cache
                    let capacity = NonZeroUsize::new(settings.cache_capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
                    let price_cache = Arc::new(Mutex::new(PriceCache::new(capacity)));

                    // Start API server
                    println!("Starting API server at: {}", bind_addr);
                    let server_handle = match core::connectivity::api::start_api_server(api_state.clone(), &bind_addr).await {
                        Ok(handle) => handle,
                        Err(e) => return Err(format!("Failed to start API server: {}", e)),
                    };

                    // Start monitor tasks
                    let monitor_handle = match core::trading::monitor::start_monitor_tasks(api_state.clone(), rpc_client.clone(), keypair.clone(), simulate_keypair.clone(), price_cache.clone(), Arc::new(settings)).await {
                        Ok(handle) => handle,
                        Err(e) => {
                            // try to shutdown server gracefully
                            let _ = server_handle.shutdown().await;
                            return Err(format!("Failed to start monitor tasks: {}", e));
                        }
                    };

                    // Save runtime handles in global state for stop command
                    RUNTIME_HANDLE.get_or_init(|| Arc::new(TokioRwLock::new(None))).write().await.replace((server_handle, monitor_handle));

                    println!("Server started at {}", bind_addr);

                    // Wait for a shutdown signal: Ctrl+C (SIGINT) or SIGTERM
                    if let Err(e) = wait_for_shutdown_signal().await {
                        eprintln!("Failed to listen for shutdown signal: {}", e);
                    }

                    // On signal, stop monitors and server (try graceful, then fallback to forced exit)
                    if let Some(handles) = RUNTIME_HANDLE.get().unwrap().write().await.take() {
                        let (server_handle, monitor_handle) = handles;
                        let stop_fut = async {
                            if let Err(e) = monitor_handle.stop().await {
                                eprintln!("Failed to stop monitor gracefully: {}", e);
                            }
                            if let Err(e) = server_handle.shutdown().await {
                                eprintln!("Failed to shutdown server gracefully: {}", e);
                            }
                        };
                        // Wait for graceful shutdown but force exit after timeout
                        tokio::select! {
                            _ = stop_fut => {}
                            _ = tokio::time::sleep(std::time::Duration::from_secs(15)) => {
                                eprintln!("Graceful shutdown timed out, forcing exit");
                                std::process::exit(1);
                            }
                        }
                    }
                    Ok(())
                }
                commands::BotCommands::Stop => {
                    // Try to stop runtime if present; otherwise call API
                    if let Some(handle_arc) = RUNTIME_HANDLE.get() {
                        let mut guard = handle_arc.write().await;
                        if let Some((server_handle, monitor_handle)) = guard.take() {
                            if let Err(e) = monitor_handle.stop().await {
                                return Err(format!("Failed to stop monitor: {}", e));
                            }
                            if let Err(e) = server_handle.shutdown().await {
                                return Err(format!("Failed to shutdown server: {}", e));
                            }
                            println!("Bot stopped");
                        } else {
                            println!("No server running in this process");
                        }
                    } else {
                        // Attempt to call API to stop if bind address is configured
                        let bind_addr = std::env::var("API_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
                        let client = reqwest::Client::new();
                        let url = format!("http://{}/bot/stop", bind_addr);
                        let resp = client.post(&url).send().await;
                        match resp {
                            Ok(_) => println!("Requested remote bot stop: {}", url),
                            Err(e) => println!("Failed to request remote bot stop: {}", e),
                        }
                    }
                    Ok(())
                }
                commands::BotCommands::Status { detailed } => {
                    if let Some(handle_arc) = RUNTIME_HANDLE.get() {
                        let guard = handle_arc.read().await;
                        if guard.is_some() {
                            println!("Bot running in-process (detailed={})", detailed);
                        } else {
                            println!("No server running in this process");
                        }
                    } else {
                        println!("No server running in this process");
                    }
                    Ok(())
                }
            }
        }
        
        // Aliases removed; use `bot start|stop|status` subcommands instead
        
        commands::Commands::Completion { shell, output } => {
            handle_completion_generation(shell.clone(), output.clone())
        }
    }
}

/// Wait for an OS shutdown signal (SIGINT/Ctrl+C or SIGTERM) and return once received.
async fn wait_for_shutdown_signal() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Listen for shutdown signals. Prefer explicit unix signals when available
    // so the process can react to SIGINT, SIGTERM, and SIGHUP. Fall back to
    // portable `ctrl_c` on non-unix platforms.

    #[cfg(unix)]
    {
        let mut sigint_stream = signal(SignalKind::interrupt())?; // SIGINT (Ctrl+C)
        let mut sigterm_stream = signal(SignalKind::terminate())?; // SIGTERM
        let mut sighup_stream = signal(SignalKind::hangup())?; // SIGHUP (terminal hangup)

        tokio::select! {
            _ = sigint_stream.recv() => {
                info!("SIGINT/Ctrl+C signal received");
            }
            _ = sigterm_stream.recv() => {
                info!("SIGTERM signal received");
            }
            _ = sighup_stream.recv() => {
                info!("SIGHUP signal received");
            }
        }
    }

    #[cfg(not(unix))]
    {
        // Portable fallback for platforms without unix signal support
        tokio::signal::ctrl_c().await?;
        info!("Ctrl+C signal received");
    }
    Ok(())
}

#[cfg(unix)]
#[cfg(test)]
mod shutdown_tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_wait_for_shutdown_signal_sigterm() {
        tokio::spawn(async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            unsafe { libc::raise(libc::SIGTERM); }
        });
        let res = wait_for_shutdown_signal().await;
        assert!(res.is_ok());
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
                .map_err(|_| core::CoreError::Storage("HOME not found".to_string()))?;
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

fn set_config_value(settings: &mut core::Settings, path: &str, value: &str) -> Result<(), core::CoreError> {
    let parts: Vec<&str> = path.split('.').collect();
    
    match parts.as_slice() {
        ["trading", "tp_percent"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| core::CoreError::Serialization("Invalid tp_percent".to_string()))?;
            settings.tp_percent = parsed;
        }
        ["trading", "sl_percent"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| core::CoreError::Serialization("Invalid sl_percent".to_string()))?;
            settings.sl_percent = parsed;
        }
        ["trading", "buy_amount"] => {
            let parsed: f64 = value.parse()
                .map_err(|_| core::CoreError::Serialization("Invalid buy_amount".to_string()))?;
            settings.buy_amount = parsed;
        }
        ["safety", "slippage_bps"] => {
            let parsed: u64 = value.parse()
                .map_err(|_| core::CoreError::Serialization("Invalid slippage_bps".to_string()))?;
            settings.slippage_bps = parsed;
        }
        _ => return Err(core::CoreError::Serialization(format!("Unknown config path: {}", path))),
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
    use std::sync::Arc;
    use async_trait::async_trait;
    use serde_json::json;
    use core_lib as core;
    
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

    #[tokio::test]
    async fn test_buy_dry_run_with_mock_rpc() {
        // Create a simple temp config file
        let mut tmpdir = std::env::temp_dir();
        tmpdir.push(format!("sol_beast_cli_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmpdir).unwrap();
        let config_path = tmpdir.join("config.toml");
        let pump_program = Pubkey::new_unique().to_string();
        let metadata_program = Pubkey::new_unique().to_string();
        let toml = format!(r#"solana_ws_urls=[]
    solana_rpc_urls=["http://localhost:8899"]
    pump_fun_program = "{}"
    metadata_program = "{}"
    tp_percent = 30.0
    sl_percent = -20.0
    timeout_secs = 3600
    cache_capacity = 16
    price_cache_ttl_secs = 60
    buy_amount = 0.1
    "#, pump_program, metadata_program);
        std::fs::write(&config_path, toml).unwrap();

        struct MockRpcClient;
        #[async_trait]
        impl core::blockchain::rpc_client::RpcClient for MockRpcClient {
            async fn get_account_info(&self, _pubkey: &str) -> Result<Option<Vec<u8>>, core::CoreError> { Ok(None) }
            async fn get_balance(&self, _pubkey: &str) -> Result<u64, core::CoreError> { Ok(1_000_000_000) }
            async fn send_transaction(&self, _transaction: &[u8]) -> Result<String, core::CoreError> { Ok("SIG".to_string()) }
            async fn confirm_transaction(&self, _signature: &str) -> Result<bool, core::CoreError> { Ok(true) }
            async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash, core::CoreError> { Ok(solana_sdk::hash::Hash::default()) }
            async fn simulate_transaction_with_config(&self, _tx: &solana_sdk::transaction::Transaction, _config: serde_json::Value) -> Result<serde_json::Value, core::CoreError> { Ok(json!({"value": null})) }
            async fn send_and_confirm_transaction(&self, _tx: &solana_sdk::transaction::Transaction) -> Result<String, core::CoreError> { Ok("SIG".to_string()) }
        }

        // Mock provider to intercept HTTP JSON-RPC calls
        struct MockProvider;
        use base64::engine::general_purpose::STANDARD as Base64Engine;
        use base64::Engine;
        use solana_sdk::pubkey::Pubkey;
        #[async_trait]
        impl core::rpc::RpcProvider for MockProvider {
            async fn send_json(&self, request: serde_json::Value) -> Result<serde_json::Value, core::CoreError> {
                let method = request.get("method").and_then(|v| v.as_str()).unwrap_or("");
                match method {
                    "getAccountInfo" => {
                        let mut data = vec![0u8; 73];
                        let global_discriminator: [u8; 8] = [0xa7, 0xe8, 0xe8, 0xb1, 0xc8, 0x6c, 0x72, 0x7f];
                        data[0..8].copy_from_slice(&global_discriminator);
                        let fee_recipient_pk = Pubkey::new_unique();
                        data[8+33..8+33+32].copy_from_slice(fee_recipient_pk.as_ref());
                        let encoded = Base64Engine.encode(&data);
                        Ok(json!({"jsonrpc":"2.0","id":1,"result": {"value": {"data": [encoded, "base64"]}}}))
                    }
                    _ => Ok(json!({"jsonrpc":"2.0","id":1,"result": null})),
                }
            }
        }
        core::rpc::set_global_json_rpc_provider(Some(Arc::new(MockProvider))).await;

        let mint = Pubkey::new_unique().to_string();
        let cli = Cli::try_parse_from(vec!["sol-beast", "--config", &config_path.to_string_lossy(), "--dry-run", "trade", "buy", &mint]).unwrap();
        let rpc_client = Arc::new(MockRpcClient{});
        let result = handle_command_with_client(cli, Some(rpc_client)).await;
        if let Err(e) = &result {
            panic!("CLI handle_command error: {}", e);
        }
    }

    #[tokio::test]
    async fn test_bot_start_and_stop_with_mock_rpc() {
        use solana_sdk::pubkey::Pubkey;
        // Create config similar to previous test
        let mut tmpdir = std::env::temp_dir();
        tmpdir.push(format!("sol_beast_cli_test_{}", std::process::id()));
        std::fs::create_dir_all(&tmpdir).unwrap();
        let config_path = tmpdir.join("config.toml");
        let pump_program = Pubkey::new_unique().to_string();
        let metadata_program = Pubkey::new_unique().to_string();
        let toml = format!(r#"solana_ws_urls=[]
    solana_rpc_urls=["http://localhost:8899"]
    pump_fun_program = "{}"
    metadata_program = "{}"
    tp_percent = 30.0
    sl_percent = -20.0
    timeout_secs = 3600
    cache_capacity = 16
    price_cache_ttl_secs = 60
    buy_amount = 0.1
    "#, pump_program, metadata_program);
        std::fs::write(&config_path, toml).unwrap();

        // Mock RPC client same as the buy test
        struct MockRpcClient;
        #[async_trait]
        impl core::blockchain::rpc_client::RpcClient for MockRpcClient {
            async fn get_account_info(&self, _pubkey: &str) -> Result<Option<Vec<u8>>, core::CoreError> { Ok(None) }
            async fn get_balance(&self, _pubkey: &str) -> Result<u64, core::CoreError> { Ok(1_000_000_000) }
            async fn send_transaction(&self, _transaction: &[u8]) -> Result<String, core::CoreError> { Ok("SIG".to_string()) }
            async fn confirm_transaction(&self, _signature: &str) -> Result<bool, core::CoreError> { Ok(true) }
            async fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash, core::CoreError> { Ok(solana_sdk::hash::Hash::default()) }
            async fn simulate_transaction_with_config(&self, _tx: &solana_sdk::transaction::Transaction, _config: serde_json::Value) -> Result<serde_json::Value, core::CoreError> { Ok(json!({"value": null})) }
            async fn send_and_confirm_transaction(&self, _tx: &solana_sdk::transaction::Transaction) -> Result<String, core::CoreError> { Ok("SIG".to_string()) }
        }

        let cli = Cli::try_parse_from(vec!["sol-beast", "--config", &config_path.to_string_lossy(), "bot", "start"]).unwrap();
        let rpc_client = Arc::new(MockRpcClient{});

        // Spawn the start in background to allow stop signal later
        let start_handle = tokio::spawn(async move {
            let result = handle_command_with_client(cli, Some(rpc_client)).await;
            if let Err(e) = result { panic!("start error: {}", e); }
        });

        // Wait a little for server to bind and tasks to spawn
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Now invoke stop; since RUNTIME_HANDLE is global in-process, use CLI stop command
        let cli2 = Cli::try_parse_from(vec!["sol-beast", "--config", &config_path.to_string_lossy(), "bot", "stop"]).unwrap();
        let result2 = handle_command_with_client(cli2, None).await;
        if let Err(e) = &result2 { panic!("stop error: {}", e); }

        // Stop background task if it's still running
        start_handle.abort();
        let _ = start_handle.await;
    }
}
