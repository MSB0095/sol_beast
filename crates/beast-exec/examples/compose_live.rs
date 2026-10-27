//! Live smoke test for the composed multi-leg arbitrage path.
//!
//! Run with a funded-or-unfunded wallet; nothing is ever sent. It quotes two
//! legs from Jupiter, fetches their instruction sets, resolves the address
//! lookup tables from mainnet, assembles the v0 transaction the executor would
//! submit, and prints the measurements that decide viability.
//!
//! ```text
//! cargo run -p beast-exec --example compose_live --features beast-rpc/live
//! ```

use beast_dex::jupiter::{JupiterClient, QuoteRequest, SwapRequest};

const WSOL: &str = "So11111111111111111111111111111111111111112";
const USDC: &str = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let amount_in: u64 = std::env::args()
        .nth(1)
        .and_then(|value| value.parse().ok())
        .unwrap_or(100_000_000);

    let jupiter = JupiterClient::new()?;
    let rpc = beast_rpc::RpcClient::new(
        vec!["https://api.mainnet-beta.solana.com".to_string()],
        "confirmed".to_string(),
        8_000,
        2,
    )?;

    let legs = [(WSOL, USDC, "WSOL -> USDC"), (USDC, WSOL, "USDC -> WSOL")];

    let mut cycle_legs = Vec::new();
    let mut amount = amount_in;
    let mut table_keys = Vec::new();
    for (index, (input, output, label)) in legs.iter().enumerate() {
        let input = input.parse::<beast_dex::Address>()?;
        let output = output.parse::<beast_dex::Address>()?;
        let mut request = QuoteRequest::new(input, output, amount);
        request.slippage_bps = 100;
        request.only_direct_routes = true;
        let quote = jupiter.quote(&request).await?;
        let min_out = quote.minimum_out()?;
        println!(
            "leg {} {label}: route={:?} minOut={min_out}",
            index,
            quote.route_labels()
        );

        let mut swap = SwapRequest::new("11111111111111111111111111111111", quote);
        swap.as_legacy_transaction = true;
        swap.wrap_and_unwrap_sol = true;
        let instructions = jupiter.swap_instructions(&swap).await?;
        println!(
            "      accounts={} alts={} payload_bytes~={}",
            instructions.swap_instruction.accounts.len(),
            instructions.address_lookup_table_addresses.len(),
            instructions.payload_bytes()
        );
        for table in &instructions.address_lookup_table_addresses {
            if !table_keys.contains(table) {
                table_keys.push(*table);
            }
        }
        amount = min_out;
        cycle_legs.push(beast_exec::compose::CycleLeg {
            instructions,
            is_final: index + 1 == legs.len(),
        });
    }

    let composed = beast_exec::compose::compose_cycle(&cycle_legs)?;
    println!(
        "\ncomposed: {} instructions, estimated_bytes={}, limit={}",
        composed.instructions.len(),
        composed.estimated_bytes,
        beast_exec::compose::MAX_TRANSACTION_BYTES
    );

    let tables = resolve(&rpc, &table_keys).await?;
    let mut rows = 0usize;
    for table in &tables {
        println!("  ALT {} -> {} addresses", table.key, table.addresses.len());
        rows += table.addresses.len();
    }
    println!("  total table rows available: {rows}");

    let wallet = solana_sdk::pubkey::Pubkey::new_unique();
    let blockhash = solana_sdk::hash::Hash::new_unique();
    let transaction = beast_exec::compose::assemble_v0(&composed, wallet, blockhash, &tables)?;
    let bytes = bincode::serialize(&transaction)?;
    println!(
        "\nv0 transaction: {} bytes (limit {}) -> {}",
        bytes.len(),
        beast_exec::compose::MAX_TRANSACTION_BYTES,
        if bytes.len() <= beast_exec::compose::MAX_TRANSACTION_BYTES {
            "FITS"
        } else {
            "TOO LARGE"
        }
    );
    println!(
        "cycle: minOut={amount} vs amount_in={amount_in} -> {} lamports",
        amount as i64 - amount_in as i64
    );
    Ok(())
}

async fn resolve(
    rpc: &beast_rpc::RpcClient,
    keys: &[beast_dex::Address],
) -> Result<Vec<beast_exec::compose::AddressLookupTable>, Box<dyn std::error::Error>> {
    let accounts = rpc.get_multiple_accounts(keys).await?;
    let mut tables = Vec::new();
    for (key, account) in keys.iter().zip(accounts) {
        let account = account.ok_or("missing ALT")?;
        let meta = 56usize;
        if account.data.len() < meta {
            return Err("ALT too small".into());
        }
        let remainder = account.data.len() - meta;
        if !remainder.is_multiple_of(32) {
            return Err("ALT data not address-aligned".into());
        }
        let count = remainder / 32;
        let addresses = account
            .data
            .get(meta..meta + count * 32)
            .ok_or("ALT range")?
            .chunks_exact(32)
            .filter_map(|bytes| beast_dex::Address::from_slice(bytes).ok())
            .collect();
        tables.push(beast_exec::compose::AddressLookupTable {
            key: *key,
            addresses,
        });
    }
    Ok(tables)
}
