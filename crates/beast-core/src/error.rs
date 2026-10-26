use thiserror::Error;

pub type Result<T> = std::result::Result<T, BeastError>;

/// Error taxonomy used across every 2.0 crate.
#[derive(Debug, Error)]
pub enum BeastError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("wallet/key error: {0}")]
    Wallet(String),
    #[error("feed error: {0}")]
    Feed(String),
    #[error("RPC error: {0}")]
    Rpc(String),
    #[error("simulation error: {0}")]
    Simulation(String),
    #[error("transaction build error: {0}")]
    Build(String),
    #[error("submission error: {0}")]
    Submit(String),
    #[error("risk control blocked the action: {0}")]
    Risk(String),
    #[error("storage error: {0}")]
    Store(String),
    #[error("API error: {0}")]
    Api(String),
    #[error("strategy error: {0}")]
    Strategy(String),
    #[error("unsupported or unverified operation: {0}")]
    Unsupported(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML error: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl BeastError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Rpc(_) | Self::Feed(_) | Self::Io(_) | Self::Submit(_)
        )
    }
}
