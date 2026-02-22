//! Weekend handling example
//!
//! Shows how the library automatically handles weekend timestamps.

use dukascopy_fx::{
    datetime, get_market_status, is_market_open, is_weekend, time::Datelike, MarketStatus, Ticker,
};

#[tokio::main]
async fn main() -> dukascopy_fx::Result<()> {
    env_logger::init();

    println!("=== Weekend Handling Example ===\n");

    let ticker = Ticker::eur_usd();

    // Timestamps around the weekend
    let timestamps = [
        ("Friday 20:00 UTC", datetime!(2025-1-3 20:00 UTC)),
        ("Friday 21:30 UTC", datetime!(2025-1-3 21:30 UTC)),
        (
            "Friday 22:30 UTC (after close)",
            datetime!(2025-1-3 22:30 UTC),
        ),
        ("Saturday 12:00 UTC", datetime!(2025-1-4 12:00 UTC)),
        ("Sunday 10:00 UTC", datetime!(2025-1-5 10:00 UTC)),
        (
            "Sunday 22:30 UTC (after open)",
            datetime!(2025-1-5 22:30 UTC),
        ),
        ("Monday 10:00 UTC", datetime!(2025-1-6 10:00 UTC)),
    ];

    // Market status analysis
    println!("Market Status Analysis:\n");
    println!(
        "{:35} {:10} {:10} {:30}",
        "Timestamp", "Weekend?", "Open?", "Status"
    );
    println!("{}", "-".repeat(90));

    for (name, ts) in &timestamps {
        let weekend = is_weekend(*ts);
        let open = is_market_open(*ts);
        let status = match get_market_status(*ts) {
            MarketStatus::Open => "Open".to_string(),
            MarketStatus::Weekend { reopens_at } => {
                format!("Weekend (opens {})", reopens_at.format("%a %H:%M"))
            }
            MarketStatus::Holiday { name, .. } => format!("Holiday: {:?}", name),
        };

        println!("{:35} {:10} {:10} {}", name, weekend, open, status);
    }

    // Fetch rates - weekend timestamps auto-adjust to Friday
    println!("\n\nFetching Rates (weekend timestamps get Friday's last data):\n");

    for (name, ts) in &timestamps {
        match ticker.rate_at(*ts).await {
            Ok(rate) => {
                let actual_day = rate.timestamp.weekday();
                let requested_day = ts.weekday();

                let note = if actual_day != requested_day {
                    format!(" (adjusted: {:?} -> {:?})", requested_day, actual_day)
                } else {
                    String::new()
                };

                println!(
                    "{:35} Rate: {} @ {}{}",
                    name,
                    rate.rate,
                    rate.timestamp.format("%Y-%m-%d %H:%M:%S"),
                    note
                );
            }
            Err(e) => {
                println!("{:35} Error: {}", name, e);
            }
        }
    }

    println!("\n\n=== Key Takeaways ===");
    println!("1. Weekend timestamps automatically return Friday's last available tick");
    println!("2. Market closes Friday ~22:00 UTC (winter) / ~21:00 UTC (summer)");
    println!("3. Market opens Sunday ~22:00 UTC (winter) / ~21:00 UTC (summer)");
    println!("4. Use is_market_open() to check before making time-sensitive decisions");
    println!("5. Use get_market_status() to get detailed info including reopen time");

    Ok(())
}
