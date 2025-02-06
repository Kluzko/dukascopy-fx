use crate::{
    parser::DukascopyParser, CurrencyExchange, CurrencyPair, DukascopyClient, DukascopyError,
};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rust_decimal::prelude::FromPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

pub struct DukascopyFxService;

impl DukascopyFxService {
    pub async fn get_exchange_rate(
        pair: &CurrencyPair,
        timestamp: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        if pair.from.len() != 3 || pair.to.len() != 3 {
            return Err(DukascopyError::InvalidCurrencyCode);
        }

        let day_of_week = timestamp.weekday();
        if day_of_week == chrono::Weekday::Sat || day_of_week == chrono::Weekday::Sun {
            let friday =
                timestamp - Duration::days((day_of_week.num_days_from_monday() + 2) as i64);
            return Self::get_last_tick_of_day(pair, friday).await;
        }

        let url = format!(
            "https://datafeed.dukascopy.com/datafeed/{}{}/{}/{:02}/{:02}/{}h_ticks.bi5",
            pair.from,
            pair.to,
            timestamp.year(),
            timestamp.month() - 1,
            timestamp.day(),
            timestamp.hour()
        );

        let decompressed_data = DukascopyClient::get_cached_data(&url).await?;
        let target_ms = timestamp.minute() as u32 * 60_000 + timestamp.second() as u32 * 1_000;

        let mut closest_tick = None;
        let mut closest_ms = 0;
        let mut min_diff = u32::MAX;

        for chunk in decompressed_data.chunks_exact(20) {
            let (ms_from_hour, ask, bid, ask_volume, bid_volume) =
                DukascopyParser::parse_tick(chunk)?;
            let diff = ms_from_hour.abs_diff(target_ms);

            if diff < min_diff {
                min_diff = diff;
                closest_ms = ms_from_hour;
                closest_tick = Some((ask, bid, ask_volume, bid_volume));
                if diff == 0 {
                    break;
                }
            }
        }

        if let Some((ask, bid, ask_volume, bid_volume)) = closest_tick {
            let price = (ask + bid) / 2.0;
            let rate = Decimal::from_f64(price)
                .ok_or(DukascopyError::Unknown("Invalid price conversion".into()))?;
            let rate = rate.round_dp_with_strategy(4, RoundingStrategy::MidpointNearestEven);

            let tick_time = timestamp
                .with_minute(0)
                .unwrap()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap()
                + Duration::milliseconds(closest_ms as i64);

            Ok(CurrencyExchange {
                pair: pair.clone(),
                rate,
                timestamp: tick_time,
                ask_volume,
                bid_volume,
            })
        } else {
            Err(DukascopyError::DataNotFound)
        }
    }

    async fn get_last_tick_of_day(
        pair: &CurrencyPair,
        date: DateTime<Utc>,
    ) -> Result<CurrencyExchange, DukascopyError> {
        let last_hour = date
            .with_hour(23)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap();
        let url = format!(
            "https://datafeed.dukascopy.com/datafeed/{}{}/{}/{:02}/{:02}/{}h_ticks.bi5",
            pair.from,
            pair.to,
            last_hour.year(),
            last_hour.month() - 1,
            last_hour.day(),
            last_hour.hour()
        );

        let decompressed_data = DukascopyClient::get_cached_data(&url).await?;

        if let Some(last_chunk) = decompressed_data.chunks_exact(20).last() {
            let (ms_from_hour, ask, bid, ask_volume, bid_volume) =
                DukascopyParser::parse_tick(last_chunk)?;
            let price = (ask + bid) / 2.0;
            let rate = Decimal::from_f64(price)
                .ok_or(DukascopyError::Unknown("Invalid price conversion".into()))?;
            let rate = rate.round_dp_with_strategy(4, RoundingStrategy::MidpointNearestEven);

            let tick_time = last_hour + Duration::milliseconds(ms_from_hour as i64);

            Ok(CurrencyExchange {
                pair: pair.clone(),
                rate,
                timestamp: tick_time,
                ask_volume,
                bid_volume,
            })
        } else {
            Err(DukascopyError::DataNotFound)
        }
    }
}
