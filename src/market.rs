//! Forex market hours utilities.
//!
//! The forex market operates 24 hours from Sunday evening to Friday evening (UTC).
//! This module provides utilities for checking market hours and handling weekends.
//!
//! # Market Hours (UTC)
//!
//! | Season | Sunday Open | Friday Close |
//! |--------|-------------|--------------|
//! | Winter | 22:00 UTC   | 22:00 UTC    |
//! | Summer | 21:00 UTC   | 21:00 UTC    |
//!
//! Sources:
//! - <https://www.dukascopy.com/swiss/english/fx-market-tools/forex-market-hours/>

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc, Weekday};

/// Market close hour on Friday (UTC) - Winter time
pub const MARKET_CLOSE_HOUR_WINTER: u32 = 22;

/// Market close hour on Friday (UTC) - Summer time (DST)
pub const MARKET_CLOSE_HOUR_SUMMER: u32 = 21;

/// Default market close hour (using winter time as conservative default)
pub const MARKET_CLOSE_HOUR: u32 = MARKET_CLOSE_HOUR_WINTER;

fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, nth: u32) -> NaiveDate {
    let first_day = NaiveDate::from_ymd_opt(year, month, 1).expect("Invalid date");
    let first_weekday = first_day.weekday().num_days_from_monday() as i64;
    let target_weekday = weekday.num_days_from_monday() as i64;

    let mut offset = target_weekday - first_weekday;
    if offset < 0 {
        offset += 7;
    }

    first_day + Duration::days(offset + 7 * (nth as i64 - 1))
}

fn is_us_dst(date: NaiveDate) -> bool {
    let year = date.year();
    let dst_start = nth_weekday_of_month(year, 3, Weekday::Sun, 2);
    let dst_end = nth_weekday_of_month(year, 11, Weekday::Sun, 1);
    date >= dst_start && date < dst_end
}

fn market_close_hour_for_date(date: NaiveDate) -> u32 {
    if is_us_dst(date) {
        MARKET_CLOSE_HOUR_SUMMER
    } else {
        MARKET_CLOSE_HOUR_WINTER
    }
}

fn market_close_hour_at(timestamp: DateTime<Utc>) -> u32 {
    market_close_hour_for_date(timestamp.date_naive())
}

/// Information about market status
#[derive(Debug, Clone, PartialEq)]
pub enum MarketStatus {
    /// Market is open
    Open,
    /// Market is closed for the weekend
    Weekend {
        /// When the market will reopen
        reopens_at: DateTime<Utc>,
    },
    /// Market is closed for a holiday
    Holiday {
        /// Name of the holiday if known
        name: Option<String>,
        /// When the market will reopen
        reopens_at: DateTime<Utc>,
    },
}

impl MarketStatus {
    /// Returns true if the market is open
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Open)
    }
}

/// Checks if the given timestamp falls on a weekend (Saturday or Sunday).
///
/// # Arguments
/// * `timestamp` - The timestamp to check
///
/// # Returns
/// `true` if the timestamp is on Saturday or Sunday
#[inline]
pub fn is_weekend(timestamp: DateTime<Utc>) -> bool {
    matches!(timestamp.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Checks if the market is open at the given timestamp.
///
/// The forex market is closed:
/// - From Friday 22:00 UTC (winter) / 21:00 UTC (summer)
/// - Until Sunday 22:00 UTC (winter) / 21:00 UTC (summer)
///
/// # Arguments
/// * `timestamp` - The timestamp to check
///
/// # Returns
/// `true` if the market is likely open
pub fn is_market_open(timestamp: DateTime<Utc>) -> bool {
    let weekday = timestamp.weekday();
    let hour = timestamp.hour();
    let close_hour = market_close_hour_at(timestamp);

    match weekday {
        // Saturday - always closed
        Weekday::Sat => false,
        // Sunday - opens at 21:00/22:00 UTC
        Weekday::Sun => hour >= close_hour,
        // Friday - closes at 21:00/22:00 UTC
        Weekday::Fri => hour < close_hour,
        // Monday through Thursday - always open
        _ => true,
    }
}

/// Gets the market status for the given timestamp.
///
/// # Arguments
/// * `timestamp` - The timestamp to check
///
/// # Returns
/// The market status including when it reopens if closed
pub fn get_market_status(timestamp: DateTime<Utc>) -> MarketStatus {
    if is_market_open(timestamp) {
        return MarketStatus::Open;
    }

    let reopens_at = next_market_open(timestamp);
    MarketStatus::Weekend { reopens_at }
}

/// Calculates when the market will next open.
///
/// # Arguments
/// * `timestamp` - The current timestamp
///
/// # Returns
/// The timestamp when the market will open
pub fn next_market_open(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    let weekday = timestamp.weekday();
    let hour = timestamp.hour();
    let close_hour = market_close_hour_at(timestamp);

    // Calculate days until Sunday
    let days_until_sunday = match weekday {
        Weekday::Fri if hour >= close_hour => 2, // Friday after close -> Sunday
        Weekday::Sat => 1,                       // Saturday -> Sunday
        Weekday::Sun if hour < close_hour => 0,  // Sunday before open -> same day
        _ => return timestamp,                   // Market is open, return current time
    };

    let open_date = timestamp.date_naive() + Duration::days(days_until_sunday);
    let open_hour = market_close_hour_for_date(open_date);
    open_date
        .and_hms_opt(open_hour, 0, 0)
        .expect("Invalid time")
        .and_utc()
}

/// Gets the last trading day before or on the given date.
///
/// If the date is a weekend, returns the previous Friday.
/// If the date is a weekday, returns the same date.
///
/// # Arguments
/// * `date` - The date to check
///
/// # Returns
/// The last trading day
pub fn last_trading_day(date: NaiveDate) -> NaiveDate {
    match date.weekday() {
        Weekday::Sat => date - Duration::days(1), // Saturday -> Friday
        Weekday::Sun => date - Duration::days(2), // Sunday -> Friday
        _ => date,
    }
}

/// Gets the last available tick time for a given timestamp.
///
/// If the timestamp is during market hours, returns the timestamp.
/// If the timestamp is on a weekend, returns the last tick time from Friday.
///
/// # Arguments
/// * `timestamp` - The timestamp to adjust
///
/// # Returns
/// The adjusted timestamp pointing to available data
pub fn last_available_tick_time(timestamp: DateTime<Utc>) -> DateTime<Utc> {
    let weekday = timestamp.weekday();
    let hour = timestamp.hour();
    let close_hour = market_close_hour_at(timestamp);

    match weekday {
        Weekday::Sat => {
            // Saturday -> Friday at market close hour - 1 (last full hour of data)
            let friday = timestamp.date_naive() - Duration::days(1);
            let friday_close_hour = market_close_hour_for_date(friday);
            friday
                .and_hms_opt(friday_close_hour - 1, 59, 59)
                .expect("Invalid time")
                .and_utc()
        }
        Weekday::Sun => {
            if hour < close_hour {
                // Sunday before market opens -> Friday
                let friday = timestamp.date_naive() - Duration::days(2);
                let friday_close_hour = market_close_hour_for_date(friday);
                friday
                    .and_hms_opt(friday_close_hour - 1, 59, 59)
                    .expect("Invalid time")
                    .and_utc()
            } else {
                // Sunday after market opens -> current time is fine
                timestamp
            }
        }
        Weekday::Fri if hour >= close_hour => {
            // Friday after close -> last tick before close
            timestamp
                .date_naive()
                .and_hms_opt(close_hour - 1, 59, 59)
                .expect("Invalid time")
                .and_utc()
        }
        _ => timestamp,
    }
}

/// Calculates the number of days to go back to get Friday from a weekend day.
///
/// # Arguments
/// * `weekday` - The current weekday
///
/// # Returns
/// Number of days to subtract to get to Friday, or 0 if not a weekend
pub fn days_to_friday(weekday: Weekday) -> i64 {
    match weekday {
        Weekday::Sat => 1,
        Weekday::Sun => 2,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    mod is_weekend {
        use super::*;

        #[test]
        fn test_saturday() {
            let sat = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap(); // Saturday
            assert!(is_weekend(sat));
        }

        #[test]
        fn test_sunday() {
            let sun = Utc.with_ymd_and_hms(2024, 1, 7, 12, 0, 0).unwrap(); // Sunday
            assert!(is_weekend(sun));
        }

        #[test]
        fn test_weekdays() {
            for day in [1, 2, 3, 4, 5] {
                // Mon-Fri
                let weekday = Utc.with_ymd_and_hms(2024, 1, day, 12, 0, 0).unwrap();
                assert!(!is_weekend(weekday), "Day {} should not be weekend", day);
            }
        }
    }

    mod is_market_open {
        use super::*;

        #[test]
        fn test_monday_midday() {
            let mon = Utc.with_ymd_and_hms(2024, 1, 8, 12, 0, 0).unwrap();
            assert!(is_market_open(mon));
        }

        #[test]
        fn test_friday_before_close() {
            let fri = Utc.with_ymd_and_hms(2024, 1, 5, 20, 0, 0).unwrap();
            assert!(is_market_open(fri));
        }

        #[test]
        fn test_friday_after_close() {
            let fri = Utc.with_ymd_and_hms(2024, 1, 5, 22, 0, 0).unwrap();
            assert!(!is_market_open(fri));
        }

        #[test]
        fn test_saturday() {
            let sat = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
            assert!(!is_market_open(sat));
        }

        #[test]
        fn test_sunday_before_open() {
            let sun = Utc.with_ymd_and_hms(2024, 1, 7, 20, 0, 0).unwrap();
            assert!(!is_market_open(sun));
        }

        #[test]
        fn test_sunday_after_open() {
            let sun = Utc.with_ymd_and_hms(2024, 1, 7, 22, 0, 0).unwrap();
            assert!(is_market_open(sun));
        }

        #[test]
        fn test_summer_friday_after_close() {
            // July is within US DST, close should be 21:00 UTC
            let fri = Utc.with_ymd_and_hms(2024, 7, 5, 21, 30, 0).unwrap();
            assert!(!is_market_open(fri));
        }

        #[test]
        fn test_summer_sunday_open_hour() {
            // July is within US DST, open should be 21:00 UTC
            let before_open = Utc.with_ymd_and_hms(2024, 7, 7, 20, 0, 0).unwrap();
            let after_open = Utc.with_ymd_and_hms(2024, 7, 7, 21, 0, 0).unwrap();

            assert!(!is_market_open(before_open));
            assert!(is_market_open(after_open));
        }
    }

    mod last_trading_day {
        use super::*;

        #[test]
        fn test_saturday() {
            let sat = NaiveDate::from_ymd_opt(2024, 1, 6).unwrap();
            let fri = last_trading_day(sat);
            assert_eq!(fri, NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());
        }

        #[test]
        fn test_sunday() {
            let sun = NaiveDate::from_ymd_opt(2024, 1, 7).unwrap();
            let fri = last_trading_day(sun);
            assert_eq!(fri, NaiveDate::from_ymd_opt(2024, 1, 5).unwrap());
        }

        #[test]
        fn test_weekday() {
            let wed = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
            assert_eq!(last_trading_day(wed), wed);
        }
    }

    mod last_available_tick_time {
        use super::*;

        #[test]
        fn test_weekday() {
            let wed = Utc.with_ymd_and_hms(2024, 1, 3, 14, 30, 0).unwrap();
            assert_eq!(last_available_tick_time(wed), wed);
        }

        #[test]
        fn test_saturday() {
            let sat = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
            let result = last_available_tick_time(sat);

            assert_eq!(result.weekday(), Weekday::Fri);
            assert_eq!(result.hour(), MARKET_CLOSE_HOUR - 1);
        }

        #[test]
        fn test_sunday_morning() {
            let sun = Utc.with_ymd_and_hms(2024, 1, 7, 10, 0, 0).unwrap();
            let result = last_available_tick_time(sun);

            assert_eq!(result.weekday(), Weekday::Fri);
        }

        #[test]
        fn test_sunday_evening() {
            let sun = Utc.with_ymd_and_hms(2024, 1, 7, 23, 0, 0).unwrap();
            let result = last_available_tick_time(sun);

            // After market opens, should return same time
            assert_eq!(result, sun);
        }

        #[test]
        fn test_friday_after_close() {
            let fri = Utc.with_ymd_and_hms(2024, 1, 5, 23, 0, 0).unwrap();
            let result = last_available_tick_time(fri);

            assert_eq!(result.hour(), MARKET_CLOSE_HOUR - 1);
        }

        #[test]
        fn test_summer_friday_after_close() {
            let fri = Utc.with_ymd_and_hms(2024, 7, 5, 22, 0, 0).unwrap();
            let result = last_available_tick_time(fri);
            assert_eq!(result.hour(), MARKET_CLOSE_HOUR_SUMMER - 1);
        }
    }

    mod days_to_friday {
        use super::*;

        #[test]
        fn test_saturday() {
            assert_eq!(days_to_friday(Weekday::Sat), 1);
        }

        #[test]
        fn test_sunday() {
            assert_eq!(days_to_friday(Weekday::Sun), 2);
        }

        #[test]
        fn test_weekday() {
            assert_eq!(days_to_friday(Weekday::Mon), 0);
            assert_eq!(days_to_friday(Weekday::Fri), 0);
        }
    }

    mod market_status {
        use super::*;

        #[test]
        fn test_open() {
            let mon = Utc.with_ymd_and_hms(2024, 1, 8, 12, 0, 0).unwrap();
            let status = get_market_status(mon);
            assert!(status.is_open());
        }

        #[test]
        fn test_weekend() {
            let sat = Utc.with_ymd_and_hms(2024, 1, 6, 12, 0, 0).unwrap();
            let status = get_market_status(sat);

            match status {
                MarketStatus::Weekend { reopens_at } => {
                    assert_eq!(reopens_at.weekday(), Weekday::Sun);
                    assert_eq!(reopens_at.hour(), MARKET_CLOSE_HOUR);
                }
                _ => panic!("Expected Weekend status"),
            }
        }
    }
}
