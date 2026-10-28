//! Discover every Jupiter Lend flashloan mint and its borrowable liquidity.
//!
//! Discovery walks the `UserBorrowPosition` accounts owned by the flashloan
//! admin, then resolves each position's mint against live reserve state. That
//! is the only correct order: a reserve without a position cannot be borrowed
//! through.
//!
//! ```text
//! cargo run -p beast-flash --example jupiter_discover
//! ```

use beast_flash::jupiter::JupiterFlashProvider;

const RPC: &str = "https://api.mainnet-beta.solana.com";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rpc_url = std::env::var("RPC_URL").unwrap_or_else(|_| RPC.to_string());
    let rpc = beast_rpc::RpcClient::new(vec![rpc_url], "confirmed".to_string(), 60_000, 2)?;
    let provider = JupiterFlashProvider::mainnet();
    let fee = provider.protocol_fee_bps(&rpc).await?;
    println!("protocol fee: {fee} bps");
    let reserves = provider.discover_reserves(&rpc).await?;
    let mut reserves = reserves;
    reserves.sort_by_key(|reserve| std::cmp::Reverse(reserve.available));
    println!(
        "borrowable mints: {} (available, in token units)\n",
        reserves.len()
    );
    for reserve in &reserves {
        let units = reserve.available as f64 / 10f64.powi(reserve.decimals as i32);
        println!(
            "  {:<44} {:>16.4}  ({} decimals, fee {} bps)",
            reserve.mint.to_string(),
            units,
            reserve.decimals,
            reserve.flash_fee_bits,
        );
    }
    Ok(())
}
