//! Time utilities and re-exports for convenient datetime handling.
//!
//! This module re-exports commonly used chrono types so users don't need
//! to add chrono as a separate dependency.

// Re-export commonly used chrono types
pub use chrono::{
    DateTime, Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike, Utc,
    Weekday,
};

/// Creates a UTC datetime from components.
///
/// This is a convenience function that's easier to use than chrono's
/// `Utc.with_ymd_and_hms()` method chain.
///
/// # Arguments
/// * `year` - Year (e.g., 2024)
/// * `month` - Month (1-12)
/// * `day` - Day of month (1-31)
/// * `hour` - Hour (0-23)
/// * `minute` - Minute (0-59)
/// * `second` - Second (0-59)
///
/// # Returns
/// `Some(DateTime<Utc>)` if valid, `None` if invalid
///
/// # Example
/// ```
/// use dukascopy_fx::time::datetime;
///
/// let dt = datetime(2024, 1, 15, 14, 30, 0).unwrap();
/// assert_eq!(dt.to_string(), "2024-01-15 14:30:00 UTC");
/// ```
#[inline]
pub fn datetime(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<DateTime<Utc>> {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
}

/// Creates a UTC datetime, panicking if invalid.
///
/// Prefer [`try_datetime_utc`] or [`datetime`] when parsing user input.
///
/// # Panics
/// Panics if the datetime components are invalid.
///
/// # Example
/// ```
/// use dukascopy_fx::time::datetime_utc;
///
/// let dt = datetime_utc(2024, 1, 15, 14, 30, 0);
/// ```
#[inline]
pub fn datetime_utc(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> DateTime<Utc> {
    datetime(year, month, day, hour, minute, second).expect("Invalid datetime components")
}

/// Creates a UTC datetime and returns `None` when components are invalid.
///
/// This is a non-panicking companion to [`datetime_utc`].
#[inline]
pub fn try_datetime_utc(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<DateTime<Utc>> {
    datetime(year, month, day, hour, minute, second)
}

/// Creates a UTC datetime from a date (at midnight).
///
/// # Example
/// ```
/// use dukascopy_fx::time::date;
///
/// let dt = date(2024, 1, 15).unwrap();
/// assert_eq!(dt.to_string(), "2024-01-15 00:00:00 UTC");
/// ```
#[inline]
pub fn date(year: i32, month: u32, day: u32) -> Option<DateTime<Utc>> {
    datetime(year, month, day, 0, 0, 0)
}

/// Returns the current UTC time.
///
/// # Example
/// ```
/// use dukascopy_fx::time::now;
///
/// let current = now();
/// println!("Current time: {}", current);
/// ```
#[inline]
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Returns a datetime N hours ago from now.
///
/// # Example
/// ```
/// use dukascopy_fx::time::hours_ago;
///
/// let one_hour_ago = hours_ago(1);
/// let yesterday = hours_ago(24);
/// ```
#[inline]
pub fn hours_ago(hours: i64) -> DateTime<Utc> {
    Utc::now() - Duration::hours(hours)
}

/// Returns a datetime N days ago from now.
///
/// # Example
/// ```
/// use dukascopy_fx::time::days_ago;
///
/// let yesterday = days_ago(1);
/// let last_week = days_ago(7);
/// ```
#[inline]
pub fn days_ago(days: i64) -> DateTime<Utc> {
    Utc::now() - Duration::days(days)
}

/// Returns a datetime N weeks ago from now.
///
/// # Example
/// ```
/// use dukascopy_fx::time::weeks_ago;
///
/// let last_week = weeks_ago(1);
/// let last_month = weeks_ago(4);
/// ```
#[inline]
pub fn weeks_ago(weeks: i64) -> DateTime<Utc> {
    Utc::now() - Duration::weeks(weeks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_datetime() {
        let dt = datetime(2024, 1, 15, 14, 30, 0).unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 0);
    }

    #[test]
    fn test_datetime_invalid() {
        assert!(datetime(2024, 13, 1, 0, 0, 0).is_none()); // Invalid month
        assert!(datetime(2024, 1, 32, 0, 0, 0).is_none()); // Invalid day
        assert!(datetime(2024, 1, 1, 25, 0, 0).is_none()); // Invalid hour
    }

    #[test]
    fn test_date() {
        let dt = date(2024, 6, 15).unwrap();
        assert_eq!(dt.hour(), 0);
        assert_eq!(dt.minute(), 0);
    }

    #[test]
    fn test_now() {
        let n = now();
        // Just verify it returns something reasonable
        assert!(n.year() >= 2024);
    }

    #[test]
    fn test_hours_ago() {
        let one_hour_ago = hours_ago(1);
        let n = now();
        let diff = n - one_hour_ago;
        // Should be approximately 1 hour (within a few seconds)
        assert!(diff.num_minutes() >= 59 && diff.num_minutes() <= 61);
    }

    #[test]
    fn test_days_ago() {
        let yesterday = days_ago(1);
        let n = now();
        let diff = n - yesterday;
        assert!(diff.num_hours() >= 23 && diff.num_hours() <= 25);
    }

    use chrono::Datelike;
    use chrono::Timelike;

    #[test]
    fn test_datetime_utc() {
        let dt = datetime_utc(2024, 1, 15, 14, 30, 0);
        assert_eq!(dt.year(), 2024);
    }

    #[test]
    fn test_try_datetime_utc() {
        assert!(try_datetime_utc(2024, 1, 15, 14, 30, 0).is_some());
        assert!(try_datetime_utc(2024, 13, 15, 14, 30, 0).is_none());
    }
}
