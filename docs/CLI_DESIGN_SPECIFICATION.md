# Sol Beast CLI Design Specification

## Overview

This document provides a comprehensive design for the Sol Beast CLI interface that exposes 100% of the core library functionality through an intuitive command-line interface. The design follows Rust CLI best practices and provides a production-ready interface for advanced traders and developers.

## CLI Framework Selection

- **Framework**: `clap 4.4` with derive feature
- **Completion**: `clap_complete 4.4` for shell completion
- **Coloring**: `colored 2.0` for enhanced output
- **Async Integration**: `tokio` for async command execution

## Command Hierarchy

### 1. Core Command Structure

```bash
sol-beast [GLOBAL_OPTIONS] <COMMAND> [COMMAND_OPTIONS] [ARGUMENTS]
```

#### Global Options
- `--config, -c <PATH>`: Configuration file path (default: ~/.config/sol_beast/config.toml)
- `--verbose, -v`: Enable verbose output
- `--quiet, -q`: Suppress non-essential output
- `--json, -j`: Output in JSON format
- `--color <auto|always|never>`: Color output (default: auto)
- `--dry-run`: Simulate operations without making changes

## Command Categories

### 2. Configuration Management (`config`)

Complete exposure of all 32+ configuration settings with intuitive subcommands.

#### 2.1 Get Configuration Values
```bash
# Get specific setting
sol-beast config get <SETTING_NAME>

# Examples:
sol-beast config get tp_percent
sol-beast config get helius_api_key
sol-beast config get solana_rpc_urls

# Get nested settings
sol-beast config get trading.tp_percent
sol-beast config get network.rpc_urls
```

#### 2.2 Set Configuration Values
```bash
# Set specific setting
sol-beast config set <SETTING_NAME> <VALUE>

# Examples:
sol-beast config set tp_percent 50.0
sol-beast config set helius_api_key "your-api-key"
sol-beast config set --array solana_rpc_urls "https://api.mainnet-beta.solana.com"
sol-beast config set --append solana_rpc_urls "https://solana-api.projectserum.com"

# Batch operations
sol-beast config set --file new_config.toml
sol-beast config set --from-env
```

#### 2.3 Configuration Display
```bash
# Show all settings
sol-beast config show

# Show with specific format
sol-beast config show --format json
sol-beast config show --format yaml
sol-beast config show --sensitive  # Hide sensitive values

# Show specific categories
sol-beast config show --category trading
sol-beast config show --category network
sol-beast config show --category wallet
```

#### 2.4 Configuration Validation
```bash
# Validate current configuration
sol-beast config validate

# Validate specific file
sol-beast config validate --file custom.toml

# Validate with dry-run
sol-beast config validate --dry-run
```

#### 2.5 Configuration Templates
```bash
# List available templates
sol-beast config templates list

# Generate from template
sol-beast config template beginner --output my_config.toml
sol-beast config template advanced --output advanced_config.toml
sol-beast config template trading --output trading_config.toml

# Custom template
sol-beast config template custom --values tp_percent=30.0,sl_percent=10.0
```

#### 2.6 Configuration History
```bash
# Show configuration history
sol-beast config history

# Restore previous configuration
sol-beast config restore --number 5
sol-beast config restore --timestamp "2024-01-01 12:00:00"
```

### 3. Wallet Operations (`wallet`)

Comprehensive wallet management with multiple import methods and security features.

#### 3.1 Wallet Connection
```bash
# Connect with various sources
sol-beast wallet connect --keypair ~/.config/solana/id.json
sol-beast wallet connect --private-key "private-key-hex"
sol-beast wallet connect --env-var PRIVATE_KEY
sol-beast wallet connect --address 1234567890abcdef

# Connect with interactive prompts
sol-beast wallet connect --interactive

# Connect with simulation mode
sol-beast wallet connect --simulate --keypair test_keypair.json
```

#### 3.2 Wallet Status and Info
```bash
# Show wallet status
sol-beast wallet status

# Show detailed information
sol-beast wallet info --verbose
sol-beast wallet info --format json

# Show balance
sol-beast wallet balance
sol-beast wallet balance --token <TOKEN_MINT>
sol-beast wallet balance --all  # Including tokens
```

#### 3.3 Wallet Management
```bash
# Export wallet (with safety prompts)
sol-beast wallet export --format json
sol-beast wallet export --format private-key
sol-beast wallet export --format seed-phrase

# Generate new wallet
sol-beast wallet generate --output ~/.config/sol_beast/wallet.json
sol-beast wallet generate --simulate  # Generate test wallet

# Remove wallet connection
sol-beast wallet disconnect --force
```

#### 3.4 Security Operations
```bash
# Verify wallet integrity
sol-beast wallet verify

# Test transaction signing
sol-beast wallet test-sign --amount 0.001

# Security audit
sol-beast wallet audit --check-permissions
sol-beast wallet audit --check-exposure
```

### 4. Trading Operations (`trade`)

Complete trading interface with strategy management and monitoring.

#### 4.1 Manual Trading
```bash
# Buy tokens
sol-beast trade buy <MINT_ADDRESS> --amount 1.0 --slippage 100
sol-beast trade buy --mint DG5g9... --sol-amount 0.5 --max-slippage 2%
sol-beast trade buy --batch-file buy_orders.json --dry-run

# Sell tokens
sol-beast trade sell <MINT_ADDRESS> --percentage 100
sol-beast trade sell --mint DG5g9... --amount 0.5 --min-profit 10%
sol-beast trade sell --all --max-slippage 1%

# Limit orders
sol-beast trade limit --buy <MINT_ADDRESS> --price 0.0001 --amount 1.0
sol-beast trade limit --sell <MINT_ADDRESS> --price 0.0002 --percentage 50
```

#### 4.2 Strategy Management
```bash
# Strategy configuration
sol-beast trade strategy set --tp-percent 50.0 --sl-percent 10.0
sol-beast trade strategy set --max-hold-time 3600 --min-liquidity 1.0

# Strategy templates
sol-beast trade strategy template conservative --output strategy.toml
sol-beast trade strategy template aggressive --tp 100 --sl 5

# Strategy validation
sol-beast trade strategy validate --config strategy.toml
```

#### 4.3 Portfolio Management
```bash
# Show holdings
sol-beast trade portfolio --format table
sol-beast trade portfolio --format json --detailed

# Portfolio analytics
sol-beast trade portfolio analytics --period 24h
sol-beast trade portfolio performance --compare-sol

# Portfolio filtering
sol-beast trade portfolio --profit-positive --min-hold-time 1h
sol-beast trade portfolio --sort-by profit --limit 10
```

#### 4.4 Trading History
```bash
# Trading history
sol-beast trade history --period 7d
sol-beast trade history --mint <MINT> --detailed
sol-beast trade history --format json --export trades.json

# Trade analysis
sol-beast trade analyze --period 30d --metrics all
sol-beast trade analyze --worst-trades --count 5
```

#### 4.5 Real-time Monitoring
```bash
# Monitor positions
sol-beast trade monitor --live --threshold 5%
sol-beast trade monitor --alerts --telegram-token <TOKEN>

# Market monitoring
sol-beast trade monitor new-coins --min-liquidity 1.0 --watchlist
sol-beast trade monitor pumpfun --live-feed
```

### 5. Bot Control (`bot`)

Complete bot lifecycle management with multiple operation modes.

#### 5.1 Bot Lifecycle
```bash
# Start bot
sol-beast bot start --mode live
sol-beast bot start --mode dry-run --config custom.toml
sol-beast bot start --background --pid-file bot.pid

# Stop bot
sol-beast bot stop --force  # Force stop
sol-beast bot stop --graceful --timeout 30s

# Restart bot
sol-beast bot restart --preserve-state
```

#### 5.2 Bot Status and Health
```bash
# Bot status
sol-beast bot status --detailed
sol-beast bot status --format json
sol-beast bot status --health-check

# Real-time status
sol-beast bot status --watch --interval 5s

# Performance metrics
sol-beast bot metrics --period 1h
sol-beast bot metrics --export metrics.json
```

#### 5.3 Mode Management
```bash
# Switch modes
sol-beast bot mode switch live
sol-beast bot mode switch dry-run --confirm

# Mode-specific operations
sol-beast bot dry-run --duration 1h --report
sol-beast bot live --enable-limit-orders --max-concurrent 5
```

#### 5.4 Bot Configuration
```bash
# Runtime configuration updates
sol-beast bot config set tp_percent 30.0 --runtime
sol-beast bot config reload --validate

# Configuration preview
sol-beast bot config preview --diff
sol-beast bot config history --limit 10
```

### 6. Protocol Operations (`protocol`)

Protocol-specific commands for pump.fun and IDL-based protocols.

#### 6.1 Pump.fun Operations
```bash
# Pump.fun specific trading
sol-beast protocol pumpfun buy --mint <MINT> --amount 1.0
sol-beast protocol pumpfun sell --percentage 100
sol-beast protocol pumpfun estimate --mint <MINT> --amount 0.5

# Market data
sol-beast protocol pumpfun market --mint <MINT>
sol-beast protocol pumpfun trending --limit 20
sol-beast protocol pumpfun new-coins --filters min-liquidity=1.0
```

#### 6.2 IDL Management
```bash
# IDL operations
sol-beast protocol idl load --path protocol.json --name "Custom Protocol"
sol-beast protocol idl validate --file protocol.json
sol-beast protocol idl list --active

# IDL-based trading
sol-beast protocol idl trade --idl custom.json --instruction buy --args <ARGS>
```

#### 6.3 Protocol Discovery
```bash
# List supported protocols
sol-beast protocol list --available
sol-beast protocol list --enabled

# Protocol info
sol-beast protocol info pumpfun --detailed
sol-beast protocol capabilities --json
```

### 7. Connectivity (`connectivity`)

Network operations and connectivity management.

#### 7.1 RPC Operations
```bash
# RPC management
sol-beast connectivity rpc set "https://api.mainnet-beta.solana.com"
sol-beast connectivity rpc test --all
sol-beast connectivity rpc rotate --interval 3600

# RPC status
sol-beast connectivity rpc status --format json
sol-beast connectivity rpc latency --benchmark
```

#### 7.2 WebSocket Management
```bash
# WebSocket operations
sol-beast connectivity ws connect --url wss://api.mainnet-beta.solana.com
sol-beast connectivity ws subscribe --program <PROGRAM_ID>
sol-beast connectivity ws status --verbose

# WebSocket monitoring
sol-beast connectivity ws monitor --filter mint=<MINT>
sol-beast connectivity ws health --check-subscriptions
```

#### 7.3 Network Testing
```bash
# Connectivity tests
sol-beast connectivity test rpc --timeout 10s
sol-beast connectivity test ws --duration 30s
sol-beast connectivity test full --comprehensive

# Performance benchmarks
sol-beast connectivity benchmark --duration 60s
sol-beast connectivity benchmark --export results.json
```

### 8. Blockchain Operations (`blockchain`)

Low-level blockchain operations and utilities.

#### 8.1 RPC Calls
```bash
# Direct RPC calls
sol-beast blockchain rpc get-account <ACCOUNT_ADDRESS>
sol-beast blockchain rpc get-slot
sol-beast blockchain rpc get-balance <WALLET_ADDRESS>

# Custom RPC calls
sol-beast blockchain rpc custom --method getProgramAccounts --params <PARAMS>
```

#### 8.2 Transaction Operations
```bash
# Transaction building
sol-beast blockchain tx build buy --mint <MINT> --amount 1.0
sol-beast blockchain tx simulate --tx <SIGNATURE>
sol-beast blockchain tx send --signed-tx <TRANSACTION>

# Transaction status
sol-beast blockchain tx status <SIGNATURE>
sol-beast blockchain tx confirm <SIGNATURE> --timeout 30s
```

#### 8.3 Account Operations
```bash
# Account management
sol-beast blockchain account info <ADDRESS>
sol-beast blockchain account history <ADDRESS> --limit 50
sol-beast blockchain account tokens <ADDRESS>
```

### 9. Tools and Utilities (`tools`)

Utility commands for advanced users and developers.

#### 9.1 Shell Completion
```bash
# Generate completions
sol-beast tools completion bash --output ~/.bash_completion
sol-beast tools completion zsh --output ~/.zfunc
sol-beast tools completion fish --output ~/.config/fish/completions

# Completion testing
sol-beast tools completion test --shell bash
```

#### 9.2 Data Export and Import
```bash
# Export functionality
sol-beast tools export config --output backup.toml
sol-beast tools export portfolio --format csv --period 30d
sol-beast tools export trades --mint <MINT> --format json

# Import functionality
sol-beast tools import config --file backup.toml --merge
sol-beast tools import watchlist --file tokens.csv
```

#### 9.3 Performance and Benchmarking
```bash
# Performance tests
sol-beast tools benchmark rpc --duration 60s
sol-beast tools benchmark memory --leak-check
sol-beast tools benchmark trading --simulate-ops 1000

# Performance monitoring
sol-beast tools monitor performance --live --metrics all
```

#### 9.4 Debugging and Diagnostics
```bash
# System diagnostics
sol-beast tools diagnose --full
sol-beast tools diagnose --network
sol-beast tools diagnose --wallet

# Debug mode
sol-beast tools debug --level trace
sol-beast tools debug --trace-rpc
sol-beast tools debug --trace-wallet-ops
```

## Command Aliases

To improve usability, provide common aliases:

```bash
# Configuration aliases
config c
wallet w
trade t
bot b
protocol p
connectivity conn
blockchain chain
tools tool

# Common operation aliases
sol-beast start  # -> sol-beast bot start
sol-beast stop   # -> sol-beast bot stop
sol-beast status # -> sol-beast bot status
sol-beast buy    # -> sol-beast trade buy
sol-beast sell   # -> sol-beast trade sell
sol-beast portfolio # -> sol-beast trade portfolio
```

## Safety Mechanisms

### 1. Configuration Validation

- Validate all configuration values before applying
- Check for conflicting settings
- Verify wallet compatibility
- Test RPC connectivity

### 2. Transaction Safety

```bash
# Pre-flight checks
sol-beast trade buy --dry-run --validate-mint <MINT>

# Slippage protection
sol-beast trade buy --max-slippage 2% --slippage-protection

# Amount validation
sol-beast trade buy --amount 1.0 --max-amount 10.0 --balance-check
```

### 3. Wallet Safety

```bash
# Confirmation prompts for sensitive operations
sol-beast wallet disconnect --confirm
sol-beast wallet export --verify-identity

# Simulation mode for testing
sol-beast trade buy --simulate --mint <MINT> --amount 0.1
```

### 4. Bot Safety

```bash
# Graceful shutdown
sol-beast bot stop --graceful --drain-positions

# Emergency controls
sol-beast bot emergency-stop --liquidate-positions
sol-beast bot emergency-disable-trading
```

## Error Handling Strategy

### 1. Error Categories

- **Configuration Errors**: Missing/invalid settings
- **Wallet Errors**: Connection, signing, balance issues
- **Network Errors**: RPC failures, connectivity issues
- **Trading Errors**: Insufficient funds, slippage, validation
- **System Errors**: Runtime, resource, permission issues

### 2. Error Output Format

```bash
# Structured error output
sol-beast trade buy --mint invalid_mint
# Output:
# ERROR: Trading error
#   Code: INVALID_MINT
#   Message: Invalid mint address format
#   Hint: Use base58 format, e.g., DG5g9...
#   Location: trade/buy.rs:45
```

### 3. Recovery Suggestions

```bash
# Provide actionable suggestions
sol-beast wallet connect --invalid-key
# Output:
# ERROR: Wallet connection failed
#   Code: INVALID_KEY
#   Suggestions:
#     - Check key format (base58, hex, or JSON)
#     - Verify file permissions (should be 600)
#     - Try: sol-beast wallet connect --help
```

## Advanced Features

### 1. Shell Completion

Comprehensive shell completion for all commands, options, and arguments:

```bash
# Generate completion for different shells
sol-beast tools completion bash
sol-beast tools completion zsh  
sol-beast tools completion fish

# Completion hints in help
sol-beast trade buy --<TAB>  # Shows all options
sol-beast config get <TAB>   # Shows available settings
```

### 2. Configuration Templates

Pre-configured templates for different use cases:

```bash
# Template categories
sol-beast config template list
# Output:
# - beginner: Safe settings for new users
# - conservative: Low-risk trading parameters
# - aggressive: High-performance settings
# - pumpfun-only: Pump.fun specific configuration
# - custom: User-defined templates
```

### 3. Batch Operations

```bash
# Batch trading
sol-beast trade batch --file orders.json --parallel 5

# Batch configuration
sol-beast config batch --file updates.toml --validate-first

# Batch monitoring
sol-beast trade monitor --batch-file watchlist.csv
```

### 4. Real-time Integration

```bash
# Live output streaming
sol-beast bot start --live-log --log-level info

# WebSocket integration
sol-beast trade monitor --websocket --real-time

# Event streaming
sol-beast tools events --filter trade.success --format json
```

## Integration with Core Library

### 1. Async Architecture

All CLI commands integrate seamlessly with the async runtime:

```rust
#[tokio::main]
async fn main() -> Result<(), CliError> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Trade { trade_command } => {
            match trade_command {
                TradeCommands::Buy { mint, amount } => {
                    let result = trade::buy(mint, amount).await?;
                    println!("{}", result);
                }
                // ... other commands
            }
        }
        // ... other command categories
    }
    
    Ok(())
}
```

### 2. Error Propagation

Systematic error handling with user-friendly messages:

```rust
impl From<CoreError> for CliError {
    fn from(error: CoreError) -> Self {
        match error {
            CoreError::Wallet(msg) => CliError::Wallet(msg),
            CoreError::Network(msg) => CliError::Network(msg),
            CoreError::Trading(msg) => CliError::Trading(msg),
            // Map all core errors to CLI errors
        }
    }
}
```

### 3. Configuration Management

Direct integration with core configuration:

```rust
pub async fn handle_config_get(key: &str) -> Result<(), CliError> {
    let settings = Settings::from_file(&cli.config_path)?;
    let value = settings.get(key)
        .ok_or_else(|| CliError::Config(format!("Setting '{}' not found", key)))?;
    
    println!("{}", value);
    Ok(())
}
```

## Performance Considerations

### 1. Efficient Command Structure

- Minimal overhead for common operations
- Lazy loading for complex features
- Efficient argument parsing with clap

### 2. Memory Management

- Streaming for large outputs (portfolio, history)
- Pagination for long lists
- Memory-efficient JSON formatting

### 3. Network Optimization

- Connection pooling for RPC calls
- Caching for frequently accessed data
- Asynchronous operations throughout

## Implementation Roadmap

### Phase 1: Core Framework (Week 1)
- [ ] Set up CLI framework with clap
- [ ] Implement basic command structure
- [ ] Add help system and examples
- [ ] Integrate with core library

### Phase 2: Configuration Management (Week 2)
- [ ] Implement config commands
- [ ] Add configuration validation
- [ ] Create configuration templates
- [ ] Add configuration history

### Phase 3: Wallet Operations (Week 3)
- [ ] Implement wallet commands
- [ ] Add security features
- [ ] Implement wallet validation
- [ ] Add simulation modes

### Phase 4: Trading Commands (Week 4)
- [ ] Implement trading operations
- [ ] Add strategy management
- [ ] Create monitoring features
- [ ] Add portfolio management

### Phase 5: Bot Control (Week 5)
- [ ] Implement bot lifecycle commands
- [ ] Add mode switching
- [ ] Create status and metrics
- [ ] Add safety mechanisms

### Phase 6: Advanced Features (Week 6)
- [ ] Add protocol-specific commands
- [ ] Implement connectivity features
- [ ] Create utility tools
- [ ] Add performance optimization

## Conclusion

This CLI design provides a comprehensive, user-friendly interface that exposes all core library functionality while maintaining safety and performance. The modular structure allows for phased implementation and easy maintenance, while the extensive command set serves both beginner and advanced users effectively.

The design follows Rust CLI best practices and integrates seamlessly with the existing core library, providing a production-ready command-line interface for the Sol Beast trading bot.