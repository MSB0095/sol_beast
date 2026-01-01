use crate::state::BuyRecord;
use chrono::Utc;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use log::error;

/// Formats a string for CSV by escaping double quotes and wrapping in quotes.
fn q(s: String) -> String {
    let escaped = s.replace('"', "\"\"");
    format!("\"{}\"", escaped)
}

/// Logs a successful buy to trades.csv.
pub fn log_buy(
    buy_rec: &BuyRecord,
) {
    let file_path = "trades.csv";
    let header = "type,mint,symbol,name,uri,image,creator,detect_time,buy_time,detect_to_buy_secs,buy_sol,buy_price_sol_per_token,buy_tokens,sell_time,stop_reason,sell_tokens,sell_sol,profit_percent,profit_sol,buy_signature,sell_signature\n";
    
    let needs_header = !Path::new(file_path).exists();
    
    match OpenOptions::new().create(true).append(true).open(file_path) {
        Ok(mut f) => {
            if needs_header {
                let _ = f.write_all(header.as_bytes());
            }
            
            let detect_to_buy = (buy_rec.buy_time - buy_rec.detect_time).num_seconds();
            let buy_sol_fmt = format!("{:.9}", buy_rec.buy_amount_sol);
            let buy_price_sol_fmt = format!("{:.18}", buy_rec.buy_price);
            
            let line = format!(
                "buy,{mint},{symbol},{name},{uri},{image},{creator},{detect_time},{buy_time},{detect_to_buy_secs},{buy_sol},{buy_price},{buy_tokens},,,,,,{buy_sig},\n",
                mint = q(buy_rec.mint.clone()),
                symbol = q(buy_rec.symbol.clone().unwrap_or_default()),
                name = q(buy_rec.name.clone().unwrap_or_default()),
                uri = q(buy_rec.uri.clone().unwrap_or_default()),
                image = q(buy_rec.image.clone().unwrap_or_default()),
                creator = q(buy_rec.creator.clone()),
                detect_time = buy_rec.detect_time.format("%+"),
                buy_time = buy_rec.buy_time.format("%+"),
                detect_to_buy_secs = detect_to_buy,
                buy_sol = buy_sol_fmt,
                buy_price = buy_price_sol_fmt,
                buy_tokens = buy_rec.buy_amount_tokens,
                buy_sig = q(buy_rec.buy_signature.clone().unwrap_or_default()),
            );
            let _ = f.write_all(line.as_bytes());
        }
        Err(e) => error!("Failed to open trades.csv for buy logging: {}", e),
    }
}

/// Logs a successful sell to trades.csv, completing the trade record.
pub fn log_sell(
    buy_rec: &BuyRecord,
    sell_time: chrono::DateTime<Utc>,
    stop_reason: String,
    sell_tokens_amount: f64,
    sell_sol: f64,
    profit_percent: f64,
    profit_sol: f64,
    tx_signature: String,
) {
    let file_path = "trades.csv";
    let header = "type,mint,symbol,name,uri,image,creator,detect_time,buy_time,detect_to_buy_secs,buy_sol,buy_price_sol_per_token,buy_tokens,sell_time,stop_reason,sell_tokens,sell_sol,profit_percent,profit_sol,buy_signature,sell_signature\n";
    
    let needs_header = !Path::new(file_path).exists();
    
    match OpenOptions::new().create(true).append(true).open(file_path) {
        Ok(mut f) => {
            if needs_header {
                let _ = f.write_all(header.as_bytes());
            }
            
            let detect_to_buy = (buy_rec.buy_time - buy_rec.detect_time).num_seconds();
            let buy_sol_fmt = format!("{:.9}", buy_rec.buy_amount_sol);
            let buy_price_sol_fmt = format!("{:.18}", buy_rec.buy_price);
            let sell_sol_fmt = format!("{:.9}", sell_sol);
            let profit_percent_fmt = format!("{:.2}", profit_percent);
            let profit_sol_fmt = format!("{:.9}", profit_sol);
            
            let line = format!(
                "sell,{mint},{symbol},{name},{uri},{image},{creator},{detect_time},{buy_time},{detect_to_buy_secs},{buy_sol},{buy_price},{buy_tokens},{sell_time},{stop_reason},{sell_tokens},{sell_sol},{profit_percent},{profit_sol},{buy_sig},{sell_sig}\n",
                mint = q(buy_rec.mint.clone()),
                symbol = q(buy_rec.symbol.clone().unwrap_or_default()),
                name = q(buy_rec.name.clone().unwrap_or_default()),
                uri = q(buy_rec.uri.clone().unwrap_or_default()),
                image = q(buy_rec.image.clone().unwrap_or_default()),
                creator = q(buy_rec.creator.clone()),
                detect_time = buy_rec.detect_time.format("%+"),
                buy_time = buy_rec.buy_time.format("%+"),
                detect_to_buy_secs = detect_to_buy,
                buy_sol = buy_sol_fmt,
                buy_price = buy_price_sol_fmt,
                buy_tokens = buy_rec.buy_amount_tokens,
                sell_time = sell_time.format("%+"),
                stop_reason = q(stop_reason),
                sell_tokens = format!("{:.6}", sell_tokens_amount),
                sell_sol = sell_sol_fmt,
                profit_percent = profit_percent_fmt,
                profit_sol = profit_sol_fmt,
                buy_sig = q(buy_rec.buy_signature.clone().unwrap_or_default()),
                sell_sig = q(tx_signature),
            );
            let _ = f.write_all(line.as_bytes());
        }
        Err(e) => error!("Failed to open trades.csv for sell logging: {}", e),
    }
}
