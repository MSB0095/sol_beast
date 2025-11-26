# Sol Beast - Technical Specification

## Table of Contents
1. [Project Overview](#project-overview)
2. [Module Architecture](#module-architecture)
3. [Core Library Exports](#core-library-exports)
4. [Blockchain Module](#blockchain-module)
5. [Configuration Module](#configuration-module)
6. [Connectivity Module](#connectivity-module)
7. [Core Modules](#core-modules)
8. [Protocols Module](#protocols-module)
9. [Trading Module](#trading-module)
10. [API Reference](#api-reference)
11. [Configuration Reference](#configuration-reference)
12. [Error Handling](#error-handling)

## Project Overview

Sol Beast is a comprehensive Solana trading bot designed for automated trading on the pump.fun platform. It features a modular architecture supporting both native CLI and WebAssembly (WASM) builds, with advanced trading strategies, real-time monitoring, and multi-wallet support.

### Key Features
- Automated token sniping and trading
- Multiple trading strategies with configurable parameters
- Real-time WebSocket monitoring
- RESTful API for bot control and monitoring
- Cross-platform support (native + WASM)
- Helius integration for high-performance transaction submission
- Advanced slippage protection and safety filters

## Module Architecture

```
core/src/
├── lib.rs                 # Main library exports and platform initialization
├── blockchain/            # Blockchain operations
│   ├── rpc_client.rs     # RPC client abstraction
│   ├── rpc.rs            # JSON-RPC provider and helpers
│   ├── rpc_helpers.rs    # RPC helper functions
│   ├── signer.rs         # Transaction signing interface
│   ├── transaction.rs    # Transaction building and results
│   └── tx_builder.rs     # Buy/sell instruction builders
├── config/               # Configuration and wallet management
│   ├── settings.rs       # Global settings and validation
│   └── wallet.rs         # Wallet management and storage
├── connectivity/         # Network connectivity
│   ├── api.rs            # REST API server
│   └── ws.rs             # WebSocket client
├── core/                 # Core data structures
│   ├── error.rs          # Error types and handling
│   ├── models.rs         # Data models and types
│   └── state.rs          # Application state
├── protocols/            # Protocol implementations
│   ├── idl.rs            # IDL-based instruction building
│   └── pumpfun.rs        # Pump.fun protocol support
└── trading/              # Trading functionality
    ├── buyer.rs          # Token buying logic
    ├── buyer_new.rs      # Enhanced buying logic
    ├── monitor.rs        # Holdings monitoring
    └── strategy.rs       # Trading strategies
```

## Core Library Exports

### Main Library Structure (`lib.rs`)

The core library exports are organized into the following modules:

```rust
pub mod core {
    pub mod error;
    pub mod models;
    pub mod state;
}

pub mod blockchain {
    pub mod rpc_client;
    pub mod rpc;
    pub mod rpc_helpers;
    pub mod signer;
    pub mod transaction;
    pub mod tx_builder;
}

pub mod trading {
    pub mod strategy;
    pub mod buyer;
    pub mod buyer_new;
    pub mod monitor;
}

pub mod config {
    pub mod settings;
    pub mod wallet;
}

pub mod protocols {
    pub mod idl;
    pub mod pumpfun;
}

pub mod connectivity {
    pub mod ws;
    pub mod api;
}
```

### Key Re-exports
- `CoreError`: Main error type
- Data models: `BondingCurveState`, `Holding`, `OffchainMetadata`, `OnchainFullMetadata`, `TradeRecord`, `UserAccount`
- Transaction types: `TransactionBuilder`, `TransactionResult`
- Wallet management: `WalletManager`, `WalletInfo`
- Trading: `TradingStrategy`, `StrategyConfig`
- Configuration: `Settings`
- Signing: All signer types
- Utility functions: `load_keypair_from_env_var`, `parse_private_key_string`
- Protocols: `SimpleIdl`

### Platform Initialization
```rust
// WASM initialization
#[wasm_bindgen]
pub fn initialize_wasm()

// Native initialization
pub fn init()
```

## Blockchain Module

### RPC Client (`rpc_client.rs`)

#### Core RPC Client Trait
```rust
#[async_trait]
pub trait RpcClient: Send + Sync {
    async fn get_account_info(&self, pubkey: &str) -> Result<Option<Vec<u8>>, CoreError>;
    async fn get_balance(&self, pubkey: &str) -> Result<u64, CoreError>;
    async fn send_transaction(&self, transaction: &[u8]) -> Result<String, CoreError>;
    async fn confirm_transaction(&self, signature: &str) -> Result<bool, CoreError>;
}
```

#### Native Implementation
- **Class**: `NativeRpcClient`
- **Constructor**: `new(url: String) -> Self`
- **Features**: Direct Solana RPC client integration

#### WASM Implementation
- **Class**: `WasmRpcClient`
- **Constructor**: `new(url: String) -> Self`
- **Features**: Browser-compatible HTTP-based RPC

#### Utility Functions
- **`parse_bonding_curve(data: &[u8]) -> Result<BondingCurveState, CoreError>`**
  - Parses pump.fun bonding curve account data
  - Extracts virtual/real token and SOL reserves
  - Returns bonding curve completion status and creator info

### RPC Provider (`rpc.rs`)

#### Core Provider Interface
```rust
#[async_trait]
pub trait RpcProvider: Send + Sync {
    async fn send_json(&self, request: Value) -> Result<Value, CoreError>;
}
```

#### Helper Functions
- **`fetch_with_provider(provider: &dyn RpcProvider, request: Value) -> Result<Value, CoreError>`**
  - Generic JSON-RPC request wrapper
  
- **`fetch_with_fallback<T: DeserializeOwned + Send + 'static>() -> Result<RpcResponse<T>, Box<dyn std::error::Error + Send + Sync>>`**
  - Implements round-robin RPC endpoint rotation
  - Automatic fallback between multiple RPC providers
  - Rate limiting and retry logic

- **Re-exported Helpers**:
  - `fetch_token_metadata()`: Get token metadata from on-chain and off-chain sources
  - `fetch_current_price()`: Calculate current token price from bonding curve
  - `fetch_transaction_details()`: Retrieve transaction information
  - `find_curve_account_by_mint()`: Locate bonding curve account for a token
  - `fetch_bonding_curve_state()`: Get bonding curve reserves and status
  - `detect_idl_for_mint()`: Auto-detect protocol IDL for token
  - `build_missing_ata_preinstructions()`: Generate ATA creation instructions
  - `fetch_global_fee_recipient()`: Get protocol fee recipient address
  - `fetch_bonding_curve_creator()`: Extract creator from bonding curve

### RPC Helpers (`rpc_helpers.rs`)

#### Key Functions

**`fetch_token_metadata()`**
```rust
pub async fn fetch_token_metadata(
    mint: &str,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<(Option<Metadata>, Option<OffchainMetadata>, Option<Vec<u8>>), Box<dyn std::error::Error + Send + Sync>>
```
- Fetches both on-chain (Metaplex) and off-chain metadata
- Handles base64 decoding and JSON parsing
- Supports HTTP requests for off-chain metadata (native builds)

**`fetch_current_price()`**
```rust
pub async fn fetch_current_price(
    mint: &str,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<f64, Box<dyn std::error::Error + Send + Sync>>
```
- Implements price caching with TTL
- Uses bonding curve reserves for price calculation
- Formula: `(virtual_sol_lamports / virtual_token_base_units) * 1e-3`
- Multiple fallback strategies for reliability

**`fetch_bonding_curve_state()`**
```rust
pub async fn fetch_bonding_curve_state(
    mint: &str,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<BondingCurveState, Box<dyn std::error::Error + Send + Sync>>
```
- Retrieves bonding curve account data
- Parses account discriminator and structure
- Returns complete bonding curve state

### Transaction Signing (`signer.rs`)

#### Core Signer Trait
```rust
#[async_trait]
pub trait Signer: Send + Sync {
    fn pubkey(&self) -> Pubkey;
    async fn sign_transaction(&self, tx: &mut Transaction, recent_blockhash: Hash) -> Result<(), CoreError>;
}
```

#### Native Implementation
- **Class**: `NativeKeypairSigner`
- **Constructor**: `new(keypair: Arc<Keypair>) -> Self`
- **Features**: Direct Solana keypair integration

#### WASM Implementation
- **Class**: `WasmStubSigner`
- **Features**: Placeholder for wallet extension integration

### Transaction Building (`transaction.rs`)

#### Transaction Builder
```rust
pub struct TransactionBuilder {
    pump_program: Pubkey,
}

impl TransactionBuilder {
    pub fn new(pump_program: String) -> Result<Self, CoreError>
    pub fn get_bonding_curve_pda(&self, mint: &str) -> Result<String, CoreError>
    pub fn calculate_token_output(&self, sol_amount: f64, virtual_sol_reserves: u64, virtual_token_reserves: u64) -> u64
    pub fn calculate_sol_output(&self, token_amount: u64, virtual_sol_reserves: u64, virtual_token_reserves: u64) -> u64
}
```

#### Price Calculation Methods
- **`calculate_token_output()`**: Constant product formula for SOL → Token conversion
- **`calculate_sol_output()`**: Constant product formula for Token → SOL conversion
- Uses bonding curve mathematics: `k = virtual_sol * virtual_token`

#### Transaction Result
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub signature: String,
    pub success: bool,
    pub error: Option<String>,
}
```

### Transaction Builder (`tx_builder.rs`)

#### Instruction Builders

**Buy Instruction**
```rust
pub fn build_buy_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    max_sol_cost: u64,
    track_volume: Option<bool>,
    user: &Pubkey,
    fee_recipient: &Pubkey,
    creator_pubkey: Option<Pubkey>,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>>
```

**Sell Instruction**
```rust
pub fn build_sell_instruction(
    program_id: &Pubkey,
    mint: &str,
    amount: u64,
    min_sol_output: u64,
    user: &Pubkey,
    fee_recipient: &Pubkey,
    creator_pubkey: Option<Pubkey>,
) -> Result<Instruction, Box<dyn std::error::Error + Send + Sync>>
```

#### Instruction Data Structures
```rust
#[derive(BorshSerialize)]
pub struct BuyArgs {
    pub amount: u64,
    pub max_sol_cost: u64,
    pub track_volume: Option<bool>,
}

#[derive(BorshSerialize)]
pub struct SellArgs {
    pub amount: u64,
    pub min_sol_output: u64,
}
```

#### Protocol Constants
- `BUY_DISCRIMINATOR`: `[102, 6, 61, 18, 1, 218, 235, 234]`
- `SELL_DISCRIMINATOR`: `[51, 230, 133, 164, 1, 127, 131, 173]`
- System program addresses for SOL, token, and fee operations

## Configuration Module

### Settings (`settings.rs`)

#### Core Configuration Structure
```rust
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Settings {
    // Blockchain endpoints
    pub solana_ws_urls: Vec<String>,
    pub solana_rpc_urls: Vec<String>,
    
    // Program addresses
    pub pump_fun_program: String,
    pub metadata_program: String,
    
    // Wallet configuration (multiple formats supported)
    pub wallet_keypair_path: Option<String>,
    pub wallet_keypair_json: Option<String>,
    pub wallet_private_key_string: Option<String>,
    pub simulate_wallet_private_key_string: Option<String>,
    
    // Trading parameters
    pub tp_percent: f64,           // Take profit percentage
    pub sl_percent: f64,           // Stop loss percentage
    pub buy_amount: f64,           // Default buy amount in SOL
    pub timeout_secs: i64,         // Position timeout
    
    // Performance and caching
    pub cache_capacity: usize,
    pub price_cache_ttl_secs: u64,
    
    // Risk management
    pub max_holded_coins: usize,
    pub enable_safer_sniping: bool,
    pub min_tokens_threshold: u64,
    pub max_sol_per_token: f64,
    pub slippage_bps: u64,
    pub min_liquidity_sol: f64,
    pub max_liquidity_sol: f64,
    
    // Advanced options
    pub bonding_curve_strict: bool,
    pub bonding_curve_log_debounce_secs: u64,
    
    // Helius integration
    pub helius_sender_enabled: bool,
    pub helius_api_key: Option<String>,
    pub helius_sender_endpoint: String,
    pub helius_min_tip_sol: f64,
    pub helius_priority_fee_multiplier: f64,
    pub helius_use_swqos_only: bool,
    pub helius_use_dynamic_tips: bool,
    pub helius_confirm_timeout_secs: u64,
    
    // RPC rotation
    pub rotate_rpc: bool,
    pub rpc_rotate_interval_secs: u64,
    
    // WebSocket configuration
    pub max_subs_per_wss: usize,
    pub sub_ttl_secs: u64,
    pub wss_subscribe_timeout_secs: u64,
    pub max_create_to_buy_secs: u64,
}
```

#### Key Methods
- **`from_toml_str(toml_str: &str) -> Result<Self, toml::de::Error>`**
  - Parse settings from TOML string
  
- **`from_file(path: &str) -> Result<Self, config::ConfigError>`** (Native only)
  - Load settings from configuration file
  
- **`from_toml_or_json(toml_str: &str) -> Result<Self, CoreError>`** (WASM only)
  - Parse TOML with JSON fallback
  
- **`save_to_file(path: &str) -> Result<(), CoreError>`**
  - Save settings to file
  
- **`merge(&mut self, other: &Settings)`**
  - Merge settings with override logic
  
- **`validate(&self) -> Result<(), CoreError>`**
  - Validate settings constraints
  
- **`get_effective_min_tip_sol(&self) -> f64`**
  - Calculate minimum tip based on routing mode

#### Utility Functions
- **`load_keypair_from_env_var(var: &str) -> Option<Vec<u8>>`**
  - Load base64-encoded keypair from environment
  
- **`parse_private_key_string(s: &str) -> Result<Vec<u8>, String>`**
  - Parse private keys in multiple formats:
    - Base58 (standard Solana format)
    - JSON array: `[1,2,3,...]`
    - Comma-separated: `1,2,3,...`

#### Default Values
```rust
fn default_buy_amount() -> f64 { 0.1 }
fn default_price_source() -> String { "wss".to_string() }
fn default_rotate_rpc() -> bool { true }
fn default_max_holded_coins() -> usize { 100 }
fn default_slippage_bps() -> u64 { 500 }
fn default_enable_safer_sniping() -> bool { false }
fn default_helius_sender_endpoint() -> String { "https://sender.helius-rpc.com/fast".to_string() }
fn default_helius_min_tip_sol() -> f64 { 0.001 }
```

### Wallet Management (`wallet.rs`)

#### Wallet Information
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletInfo {
    pub address: String,
    pub connected: bool,
    pub balance: Option<u64>, // lamports
}
```

#### Wallet Manager
```rust
pub struct WalletManager {
    current_wallet: Option<WalletInfo>,
}

impl WalletManager {
    pub fn new() -> Self
    pub fn connect_wallet(&mut self, address: String) -> Result<(), CoreError>
    pub fn disconnect_wallet(&mut self)
    pub fn get_current_wallet(&self) -> Option<&WalletInfo>
    pub fn is_connected(&self) -> bool
    pub fn get_wallet_address(&self) -> Option<&str>
    pub fn update_balance(&mut self, balance: u64)
}
```

#### Storage Interface (Platform-specific)

**WASM Storage** (`storage` module)
```rust
pub fn save_user_account(account: &UserAccount) -> Result<(), CoreError>
pub fn load_user_account(wallet_address: &str) -> Result<Option<UserAccount>, CoreError>
```
- Uses browser `localStorage`
- Origin-scoped storage
- No private keys stored (security consideration)

**Native Storage** (`storage` module)
```rust
pub fn save_user_account(account: &UserAccount) -> Result<(), CoreError>
pub fn load_user_account(wallet_address: &str) -> Result<Option<UserAccount>, CoreError>
```
- File-based storage in `~/.sol_beast_data/`
- JSON format with pretty printing

## Connectivity Module

### REST API Server (`api.rs`)

#### Server Setup
```rust
pub fn create_router(state: ApiState) -> Router
```
- Creates Axum-based HTTP server
- CORS enabled for web interface
- JSON API responses

#### API State Management
```rust
pub struct ApiState {
    pub settings: Arc<Mutex<Settings>>,
    pub stats: Arc<Mutex<BotStats>>,
    pub bot_control: Arc<BotControl>,
    pub detected_coins: Arc<Mutex<Vec<DetectedCoin>>>,
    pub trades: Arc<Mutex<Vec<TradeRecord>>>,
}
```

#### Bot Control Structures
```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BotRunningState {
    Stopped,
    Starting,
    Running,
    Stopping,
}

#[derive(Clone, Debug, serde::Serialize, serde:: Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum BotMode {
    DryRun,
    Real,
}
```

#### API Endpoints

**Health and Status**
- `GET /health` - Server health check
- `GET /stats` - Bot statistics and status
- `GET /bot/state` - Current bot running state and mode

**Settings Management**
- `GET /settings` - Get current settings
- `POST /settings` - Update settings (with validation)

**Bot Control**
- `POST /bot/start` - Start bot
- `POST /bot/stop` - Stop bot
- `POST /bot/mode` - Change bot mode (dry-run/real)

**Data Retrieval**
- `GET /logs` - Get recent log entries
- `GET /detected-coins` - Get detected new coins
- `GET /trades` - Get trade history

#### Log Management
```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub details: Option<String>,
}
```

#### Data Structures
```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DetectedCoin {
    pub mint: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub image: Option<String>,
    pub creator: String,
    pub bonding_curve: String,
    pub detected_at: String,
    pub metadata_uri: Option<String>,
    pub buy_price: Option<f64>,
    pub status: String,
}
```

#### Bot Statistics
```rust
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BotStats {
    pub total_buys: u64,
    pub total_sells: u64,
    pub total_profit: f64,
    pub current_holdings: Vec<HoldingWithMint>,
    pub uptime_secs: u64,
    pub last_activity: String,
    pub running_state: Option<String>,
    pub mode: Option<String>,
    pub runtime_mode: Option<String>,
}
```

### WebSocket Client (`ws.rs`)

#### Connection Management
```rust
pub async fn run_ws(
    wss_url: &str,
    tx: mpsc::Sender<String>,
    seen: Arc<Mutex<LruCache<String, ()>>>,
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    mut control_rx: mpsc::Receiver<WsRequest>,
    _settings: Arc<Settings>,
) -> Result<(), Box<dyn std::error::Error>>
```

#### WebSocket Request Types
```rust
#[derive(Debug)]
pub enum WsRequest {
    Subscribe {
        account: String,
        mint: String,
        resp: oneshot::Sender<Result<u64, String>>,
    },
    Unsubscribe {
        sub_id: u64,
        resp: oneshot::Sender<Result<(), String>>,
    },
    GetHealth {
        resp: oneshot::Sender<WsHealth>,
    },
}
```

#### Health Monitoring
```rust
#[derive(Debug, Clone)]
pub struct WsHealth {
    pub active_subs: usize,
    pub pending_subs: usize,
    pub recent_timeouts: usize,
    pub is_healthy: bool,
}
```

#### Features
- **Automatic reconnection** with ping/pong
- **Message parsing** for new token events
- **Duplicate detection** using LRU cache
- **Price cache updates** for new tokens
- **Subscription management** with timeouts

## Core Modules

### Error Handling (`error.rs`)

#### Core Error Types
```rust
#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Wallet error: {0}")]
    Wallet(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Serialization error: {0}")]
    Serialization(String),
    
    #[error("Invalid configuration: {0}")]
    Config(String),
    
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Not authorized: {0}")]
    Unauthorized(String),
    
    #[error("Invalid keypair: {0}")]
    InvalidKeypair(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}
```

#### Error Conversions
- `serde_json::Error` → `CoreError::Serialization`
- `bs58::decode::Error` → `CoreError::Parse`
- `solana_sdk::pubkey::ParsePubkeyError` → `CoreError::Parse`
- `std::io::Error` → `CoreError::Storage`
- `solana_client::client_error::ClientError` → `CoreError::Rpc`
- WASM: `CoreError` → `wasm_bindgen::JsValue`

### Data Models (`models.rs`)

#### Bonding Curve State
```rust
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
    pub creator: Option<String>,
}

impl BondingCurveState {
    pub fn spot_price_sol_per_token(&self) -> Option<f64>
}
```
- **Price calculation**: `(virtual_sol_reserves / virtual_token_reserves) * 1e-3`
- Handles SOL lamports to token base units conversion

#### User Account Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAccount {
    pub wallet_address: String,
    pub created_at: DateTime<Utc>,
    pub last_active: DateTime<Utc>,
    pub total_trades: u64,
    pub total_profit_loss: f64,
    pub settings: UserSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub tp_percent: f64,
    pub sl_percent: f64,
    pub timeout_secs: i64,
    pub buy_amount: f64,
    pub max_held_coins: usize,
    pub enable_safer_sniping: bool,
    pub min_tokens_threshold: u64,
    pub max_sol_per_token: f64,
    pub slippage_bps: u64,
}
```

#### Token Holdings
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Holding {
    pub mint: String,
    pub amount: u64,
    pub buy_price: f64,
    pub buy_time: DateTime<Utc>,
    pub metadata: Option<OffchainMetadata>,
    pub onchain_raw: Option<Vec<u8>>,
    pub onchain: Option<OnchainFullMetadata>,
}
```

#### Metadata Structures
```rust
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct OffchainMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub description: Option<String>,
    pub image: Option<String>,
    #[serde(flatten)]
    pub extras: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnchainFullMetadata {
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    pub seller_fee_basis_points: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<Vec<u8>>,
}
```

#### Trade Records
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TradeRecord {
    pub mint: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub image: Option<String>,
    pub trade_type: String, // "buy" or "sell"
    pub timestamp: DateTime<Utc>,
    pub tx_signature: Option<String>,
    pub amount_sol: f64,
    pub amount_tokens: f64,
    pub price_per_token: f64,
    pub profit_loss: Option<f64>,
    pub profit_loss_percent: Option<f64>,
    pub reason: Option<String>,
}
```

#### RPC Response Types
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse<T> {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub result: Option<T>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}
```

#### Strategy Configuration
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub tp_percent: f64,
    pub sl_percent: f64,
    pub timeout_secs: i64,
    pub enable_safer_sniping: bool,
    pub min_tokens_threshold: u64,
    pub max_sol_per_token: f64,
}
```

#### Utility Types
```rust
// Price cache using LRU
pub type PriceCache = LruCache<String, (Instant, f64)>;

// Protocol trait
pub trait Protocol {
    fn program_id(&self) -> solana_sdk::pubkey::Pubkey;
    fn name(&self) -> String;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolData {
    pub name: String,
    pub version: String,
}
```

### State Management (`state.rs`)

#### Buy Records
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuyRecord {
    pub mint: String,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub uri: Option<String>,
    pub image: Option<String>,
    pub creator: String,
    pub detect_time: chrono::DateTime<Utc>,
    pub buy_time: chrono::DateTime<Utc>,
    pub buy_amount_sol: f64,
    pub buy_amount_tokens: u64,
    pub buy_price: f64,
}
```

## Protocols Module

### IDL-Based Instruction Building (`idl.rs`)

#### Simple IDL Structure
```rust
#[derive(Debug, Clone)]
pub struct SimpleIdl {
    pub address: Pubkey,
    pub raw: Value,
}

impl SimpleIdl {
    pub fn from_value(raw: Value) -> Result<Self, CoreError>
    pub fn from_str(s: &str) -> Result<Self, CoreError>
    pub fn load_from(path: &str) -> Result<Self, CoreError> // Native only
    pub fn build_accounts_for(&self, instr_name: &str, context: &HashMap<String, Pubkey>) -> Result<Vec<AccountMeta>, CoreError>
}
```

#### Account Resolution
The IDL builder supports multiple account resolution strategies:

**Fixed Addresses**
- System program: `11111111111111111111111111111111`
- Token program: `TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA`
- Associated token program: `ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL`

**PDA Calculation**
- Seeds: `const`, `account` (from context)
- Program resolution: `const`, `account`, or current IDL address
- Automatic context updates for dependent accounts

**Special Account Types**
- `associated_user`: Derived from user + mint
- `fee_recipient`: From context or global PDA
- `program`: Current IDL address

#### IDL Loading
```rust
pub fn load_all_idls() -> Result<HashMap<String, SimpleIdl>, CoreError>
```
- Searches multiple paths for IDL files:
  - `pumpfun.json`
  - `pumpfunamm.json`
  - `pumpfunfees.json`
- Supports various directory structures
- Logs warnings for missing files

### Pump.fun Protocol (`pumpfun.rs`)

#### Protocol Implementation
```rust
pub struct PumpfunProtocol {
    program_id: Pubkey,
}

impl PumpfunProtocol {
    pub fn new(program_id: Pubkey) -> Self
}

impl Protocol for PumpfunProtocol {
    fn program_id(&self) -> Pubkey
    fn name(&self) -> String
}
```

#### Protocol IDL Details
The pump.fun IDL (`pumpfun.json`) defines:

**Program Address**: `6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P`

**Key Instructions**:
1. **`buy`**: Purchase tokens from bonding curve
2. **`sell`**: Sell tokens to bonding curve  
3. **`buy_exact_sol_in`**: Buy exact SOL amount
4. **`create`**: Create new token and bonding curve
5. **`migrate`**: Migrate to Raydium when complete

**Account Types**:
- `BondingCurve`: Token reserves and state
- `Global`: Protocol global configuration
- `FeeConfig`: Fee structure and recipients
- `UserVolumeAccumulator`: User trading volume tracking
- `GlobalVolumeAccumulator`: Protocol-wide volume tracking

**Event Types**:
- `TradeEvent`: Buy/sell transactions
- `CreateEvent`: Token creation
- `CompleteEvent`: Bonding curve completion

## Trading Module

### Token Buying (`buyer.rs` & `buyer_new.rs`)

#### Main Buy Function
```rust
pub async fn buy_token(
    mint: &str,
    sol_amount: f64,
    is_real: bool,
    keypair: Option<Arc<dyn CoreSigner>>,
    simulate_keypair: Option<Arc<dyn CoreSigner>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
) -> Result<Holding, Box<dyn std::error::Error + Send + Sync>>
```

#### Safety Checks (Safer Sniping)
```rust
if settings.enable_safer_sniping {
    // Price validation
    if token_amount < settings.min_tokens_threshold {
        return Err(format!("Token amount below minimum threshold").into());
    }
    if buy_price_sol > settings.max_sol_per_token {
        return Err(format!("Price exceeds maximum").into());
    }
    
    // Liquidity validation
    if let Ok(state) = fetch_bonding_curve_state(mint, rpc_client, settings).await {
        let real_sol = state.real_sol_reserves as f64 / 1_000_000_000.0;
        if real_sol < settings.min_liquidity_sol {
            return Err(format!("Liquidity too low").into());
        }
        if real_sol > settings.max_liquidity_sol {
            return Err(format!("Liquidity too high").into());
        }
    }
}
```

#### Transaction Building Process

**Real Trading Mode**:
1. **RPC Client Setup**: Native Solana RPC client
2. **IDL Detection**: Auto-detect protocol IDL for mint
3. **Context Building**: Create account context map
4. **Instruction Building**: Use IDL or fallback to hardcoded builder
5. **Pre-instructions**: Generate ATA creation if needed
6. **Transaction Execution**: 
   - Helius Sender (if enabled): High-performance submission
   - Standard Solana client: Direct RPC submission

**Dry Run Mode**:
1. **Simulation Setup**: Ephemeral keypair
2. **Instruction Building**: Same as real mode
3. **Transaction Simulation**: Test without actual submission
4. **Result Analysis**: Check for expected errors vs actual issues

#### Price Calculation
```rust
let buy_price_sol = fetch_current_price(mint, &price_cache, rpc_client, settings).await?;
let token_amount = ((sol_amount / buy_price_sol) * 1_000_000.0) as u64;
```

#### Slippage Protection
```rust
let base_cost_lamports = (sol_amount * 1_000_000_000.0) as u64;
let slippage_multiplier = 1.0 + (settings.slippage_bps as f64 / 10000.0);
let max_sol_cost_with_slippage = (base_cost_lamports as f64 * slippage_multiplier) as u64;
```

#### Instruction Data Encoding
```rust
let mut d = BUY_DISCRIMINATOR.to_vec();
d.extend(borsh::to_vec(&BuyArgs { 
    amount: token_amount, 
    max_sol_cost: max_sol_cost_with_slippage, 
    track_volume: Some(false) 
}).unwrap());
```

### Holdings Monitoring (`monitor.rs`)

#### Monitor Entry Point
```rust
pub async fn monitor_holdings(
    holdings: Arc<Mutex<HashMap<String, Holding>>>,
    price_cache: Arc<Mutex<PriceCache>>,
    rpc_client: Arc<dyn CoreRpcClient>,
    is_real: bool,
    keypair: Option<Arc<dyn crate::Signer>>,
    simulate_keypair: Option<Arc<dyn crate::Signer>>,
    settings: Arc<Settings>,
    trades_map: Arc<Mutex<HashMap<String, BuyRecord>>>,
    ws_control_senders: Arc<Vec<mpsc::Sender<WsRequest>>>,
    sub_map: Arc<Mutex<HashMap<String, (usize, u64)>>>,
    _next_wss_sender: Arc<AtomicUsize>,
    trades_list: Arc<tokio::sync::Mutex<Vec<TradeRecord>>>,
    bot_control: Arc<BotControl>,
)
```

#### Monitoring Loop
- **5-second interval** for price updates
- **Running state check** for graceful shutdown
- **Holdings snapshot** for concurrent access
- **Individual holding monitoring** with error isolation

#### Single Holding Monitor
```rust
async fn monitor_single_holding(
    mint: &str,
    holding: &Holding,
    price_cache: &Arc<Mutex<PriceCache>>,
    rpc_client: &Arc<dyn CoreRpcClient>,
    settings: &Arc<Settings>,
    // ... other parameters
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
```

#### Price and P&L Calculation
```rust
// Update price cache
let price = crate::rpc_helpers::fetch_current_price(mint, price_cache, rpc_client, settings).await?;

// Calculate metrics
let amount_tokens = holding.amount as f64 / 1_000_000.0;
let current_value = amount_tokens * price;
let cost_basis = holding.buy_price * amount_tokens;
let pnl = current_value - cost_basis;
let pnl_percent = if cost_basis > 0.0 {
    (pnl / cost_basis) * 100.0
} else {
    0.0
};
```

#### Trade Record Updates
- **Regular price updates** added to trades list
- **1000 record limit** for memory management
- **UI-friendly format** with timestamps and P&L

### Trading Strategies (`strategy.rs`)

#### Strategy Configuration
```rust
pub struct TradingStrategy {
    config: StrategyConfig,
}

impl TradingStrategy {
    pub fn new(config: StrategyConfig) -> Self
    pub fn config(&self) -> &StrategyConfig
    pub fn update_config(&mut self, config: StrategyConfig)
}
```

#### Buy Decision Logic
```rust
pub fn should_buy(
    &self,
    curve_state: &BondingCurveState,
    current_price: f64,
) -> Result<bool, CoreError>

// Safety checks when enable_safer_sniping = true:
// - Price validation against max_sol_per_token
// - Minimum liquidity check (real_sol_reserves)
// - Bonding curve completion status
```

#### Sell Decision Logic
```rust
pub fn should_sell(
    &self,
    holding: &Holding,
    current_price: f64,
) -> Result<Option<String>, CoreError>

// Sell conditions:
// 1. Take Profit: price_change >= tp_percent
// 2. Stop Loss: price_change <= sl_percent  
// 3. Timeout: holding_duration >= timeout_secs
```

#### Profit/Loss Calculation
```rust
pub fn calculate_profit_loss(
    &self,
    holding: &Holding,
    sell_price: f64,
    buy_amount_sol: f64,
) -> (f64, f64)

// Returns: (profit_loss_amount, profit_loss_percent)
```

#### Default Strategy Configuration
```rust
impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            tp_percent: 30.0,           // 30% take profit
            sl_percent: -20.0,          // 20% stop loss
            timeout_secs: 3600,         // 1 hour timeout
            enable_safer_sniping: true,  // Enable safety checks
            min_tokens_threshold: 1_000_000, // Minimum 1 token
            max_sol_per_token: 0.0001,  // Max price threshold
        }
    }
}
```

## API Reference

### REST API Endpoints

#### Health and Monitoring
```
GET /health
Response: {
  "status": "ok",
  "timestamp": "2025-11-26T00:49:44Z"
}

GET /stats  
Response: {
  "total_buys": 0,
  "total_sells": 0,
  "total_profit": 0.0,
  "current_holdings": [...],
  "uptime_secs": 3600,
  "last_activity": "2025-11-26T00:49:44Z",
  "running_state": "running",
  "mode": "real"
}

GET /bot/state
Response: {
  "running_state": "running",
  "mode": "real"
}
```

#### Settings Management
```
GET /settings
Response: Settings JSON object

POST /settings
Body: Partial Settings JSON
Response: {
  "status": "success",
  "message": "Settings updated successfully"
}
```

#### Bot Control
```
POST /bot/start
Response: {
  "status": "success", 
  "message": "Bot is starting"
}

POST /bot/stop
Response: {
  "status": "success",
  "message": "Bot is stopping"
}

POST /bot/mode
Body: { "mode": "dry-run" }
Response: {
  "status": "success",
  "mode": "dry-run"
}
```

#### Data Retrieval
```
GET /logs
Response: {
  "logs": [
    {
      "timestamp": "2025-11-26T00:49:44Z",
      "level": "info",
      "message": "Bot started",
      "details": null
    }
  ]
}

GET /detected-coins
Response: [...DetectedCoin array]

GET /trades
Response: [...TradeRecord array]
```

### WebSocket Protocol

#### Connection Setup
- Connect to WebSocket URL from settings
- Automatic ping/pong for connection health
- Message parsing for new token events

#### Subscription Management
```rust
// Subscribe to new tokens
{
  "method": "subscribeNewToken",
  "keys": ["account_address"]
}

// Unsubscribe  
{
  "method": "unsubscribe",
  "id": subscription_id
}

// Health check
{
  "method": "getHealth"
}
```

#### Message Types
- **newToken**: New token creation events
- **pong**: Ping response
- **error**: Error messages

## Configuration Reference

### Required Configuration
```toml
# Blockchain endpoints
solana_ws_urls = ["wss://api.mainnet-beta.solana.com"]
solana_rpc_urls = ["https://api.mainnet-beta.solana.com"]

# Program addresses
pump_fun_program = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P"
metadata_program = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"

# Trading parameters
tp_percent = 30.0
sl_percent = -20.0
buy_amount = 0.1
timeout_secs = 3600

# Performance
cache_capacity = 1000
price_cache_ttl_secs = 30
```

### Optional Configuration
```toml
# Wallet (choose one format)
wallet_keypair_path = "/path/to/keypair.json"
wallet_keypair_json = '{"[0,1,2,...]": []}'
wallet_private_key_string = "base58_private_key"

# Risk management
enable_safer_sniping = true
min_tokens_threshold = 1000000
max_sol_per_token = 0.0001
slippage_bps = 500
min_liquidity_sol = 0.1
max_liquidity_sol = 10.0

# Advanced options
max_holded_coins = 100
bonding_curve_strict = false
bonding_curve_log_debounce_secs = 300

# Helius integration
helius_sender_enabled = false
helius_api_key = "your_api_key"
helius_sender_endpoint = "https://sender.helius-rpc.com/fast"
helius_min_tip_sol = 0.001
helius_priority_fee_multiplier = 1.2
helius_use_swqos_only = false
helius_use_dynamic_tips = true
helius_confirm_timeout_secs = 15

# RPC rotation
rotate_rpc = true
rpc_rotate_interval_secs = 60

# WebSocket settings
max_subs_per_wss = 4
sub_ttl_secs = 900
wss_subscribe_timeout_secs = 6
max_create_to_buy_secs = 6
```

### Environment Variables
```bash
# Alternative wallet loading
export SOL_BEAST_PRIVATE_KEY="base58_private_key"
export SOL_BEAST_KEYPAIR_PATH="/path/to/keypair.json"

# Configuration file path
export SOL_BEAST_CONFIG_PATH="/path/to/config.toml"

# Helius API key
export HELIUS_API_KEY="your_api_key"
```

## Error Handling

### Error Types and Handling

#### Core Error Categories
1. **Wallet Errors**: Keypair issues, connection problems
2. **Transaction Errors**: Signing failures, submission errors
3. **RPC Errors**: Network issues, rate limiting, invalid responses
4. **Serialization Errors**: JSON parsing, data corruption
5. **Configuration Errors**: Invalid settings, missing parameters
6. **Storage Errors**: File system issues, permissions
7. **Network Errors**: Connectivity problems, timeouts

#### Error Recovery Strategies
- **RPC Fallback**: Automatic endpoint rotation
- **Rate Limiting**: Exponential backoff on 429 responses
- **Transaction Retry**: Built-in retry logic with different parameters
- **Graceful Degradation**: Continue operation with reduced functionality

#### Common Error Scenarios
```rust
// Price fetching failures
if price.is_err() {
    log::warn!("Failed to fetch price for {}: {}", mint, price_err);
    continue; // Skip this holding update
}

// Transaction simulation failures
if let Some(ref err) = simulation.value.err {
    if err_str.contains("AccountNotFound") {
        // Expected for dry-run mode
        log::info!("Dry run completed successfully");
    } else {
        log::warn!("Transaction simulation failed: {:?}", err);
    }
}
```

### Logging and Monitoring
- **Structured logging** with log levels (debug, info, warn, error)
- **Bot control integration** with log management
- **API endpoint** for log retrieval
- **Automatic log rotation** and size limits

This technical specification provides a comprehensive foundation for implementing a CLI interface to interact with all the trading bot's functionality. The modular architecture allows for targeted implementation of specific features while maintaining the overall system integrity and performance characteristics.