use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GraphQLQuery {
    pub query: String,
    pub variables: serde_json::Value,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GraphQLResponse<T> {
    pub data: Option<T>,
    pub errors: Option<Vec<GraphQLError>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GraphQLError {
    pub message: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ShyftTransaction {
    pub signature: String,
    pub instructions: Vec<ShyftInstruction>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ShyftInstruction {
    #[serde(rename = "programId")]
    pub program_id: String,
    pub data: String,
    pub accounts: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NewTokenSubscriptionResponse {
    pub transaction: Vec<ShyftTransaction>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ShyftAccountUpdate {
    pub pubkey: String,
    pub data: String, // Base64 encoded data
    pub lamports: u64,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct AccountSubscriptionResponse {
    pub account: Vec<ShyftAccountUpdate>,
}
