use crate::settings::Settings;
use serde_json::json;

pub struct ShyftService {
    pub api_key: String,
    pub graphql_url: String,
}

impl ShyftService {
    pub fn new(settings: &Settings) -> Option<Self> {
        let api_key = settings.shyft_api_key.clone()?;
        Some(Self {
            api_key,
            graphql_url: settings.shyft_graphql_url.clone(),
        })
    }

    pub fn get_new_token_subscription_query(&self, program_id: &str) -> String {
        // Subscription for new transactions on the pump.fun program
        // We filter by programId. Further filtering for "create" instruction 
        // (discriminator 181ec828051c0777) happens client-side or if we can add a data filter.
        // Note: Shyft GraphQL might support data filtering, but formats vary.
        let query = format!(r#"
            subscription NewTokens {{
                Transaction(
                    where: {{
                        instructions: {{
                            programId: {{ _eq: "{}" }}
                        }}
                    }}
                ) {{
                    signature
                    instructions {{
                        programId
                        data
                        accounts
                    }}
                }}
            }}
        "#, program_id);
        
        let payload = json!({
            "query": query,
            "variables": {}
        });
        
        payload.to_string()
    }

    pub fn get_account_subscription_query(&self, pubkeys: &[String]) -> String {
        let pubkeys_str = pubkeys.iter().map(|k| format!("\"{}\"", k)).collect::<Vec<_>>().join(",");
        let query = format!(r#"
            subscription PriceUpdates {{
                Account(
                    where: {{
                        pubkey: {{ _in: [{}] }}
                    }}
                ) {{
                    pubkey
                    data
                    lamports
                }}
            }}
        "#, pubkeys_str);

        let payload = json!({
            "query": query,
            "variables": {}
        });

        payload.to_string()
    }
}
