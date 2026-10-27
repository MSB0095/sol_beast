//! Enumerate Kamino flash reserves on-chain.
//!
//! This is the discovery path that makes every asset usable, not just WSOL:
//! reserves are looked up by mint, decoded from their 8624-byte account, and
//! filtered by status, availability and a valid (non-disabled) flash fee.
//!
//! `getProgramAccounts` is a heavy call that some free tiers reject, so point
//! it at an endpoint that allows it:
//!
//! ```text
//! RPC_URL="https://mainnet.helius-rpc.com/?api-key=..." \
//!   cargo run -p beast-flash --example discover
//! MINT=EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v \
//!   cargo run -p beast-flash --example discover
//! ```
//!
//! Read-only: nothing is sent.

use beast_dex::Address;
use beast_flash::{kamilend::KaminoFlashProvider, FlashReserve};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let mint = std::env::var("MINT").ok();
    let rpc = beast_rpc::RpcClient::new(vec![url], "confirmed".to_string(), 30_000, 1)?;
    let provider = KaminoFlashProvider::mainnet();

    let mut reserves = match &mint {
        Some(mint) => {
            let mint = Address::from_str(mint)?;
            println!("reserves for mint {mint}");
            provider.reserves_for_mint(&rpc, &mint).await?
        }
        None => {
            println!("all KLend reserves");
            provider.discover_reserves(&rpc).await?
        }
    };
    reserves.sort_by_key(|reserve| std::cmp::Reverse(reserve.available));

    let enabled = reserves.iter().filter(|r| r.flash_enabled()).count();
    println!(
        "decoded {} reserves, {} flash-enabled\n",
        reserves.len(),
        enabled
    );
    println!(
        "{:<46} {:<46} {:>6} {:>16} {:>12} {:>8}",
        "reserve", "mint", "status", "available", "fee ppm", "decimals"
    );
    for reserve in &reserves {
        report(reserve);
    }

    let best = reserves
        .iter()
        .filter(|reserve| reserve.flash_enabled())
        .max_by_key(|reserve| reserve.available);
    if let Some(best) = best {
        println!("\ndeepest borrowable reserve: {}", best.reserve);
        println!("  market authority: {}", best.lending_market_authority);
        println!("  supply vault    : {}", best.supply_vault);
    }
    Ok(())
}

fn report(reserve: &FlashReserve) {
    let fee_ppm = reserve.flash_fee_bits as f64 / (1u64 << 60) as f64 * 1e6;
    let fee = if reserve.flash_fee_bits == u64::MAX {
        "disabled".to_string()
    } else {
        format!("{fee_ppm:.4}")
    };
    println!(
        "{:<46} {:<46} {:>6} {:>16} {:>12} {:>8}",
        reserve.reserve.to_string(),
        reserve.mint.to_string(),
        reserve.status,
        reserve.available,
        fee,
        reserve.decimals
    );
}
