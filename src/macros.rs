//! Convenient macros for the library.

/// Creates a UTC datetime with a concise syntax.
///
/// # Formats
///
/// ```
/// use dukascopy_fx::datetime;
///
/// // Full datetime
/// let dt = datetime!(2024-1-15 14:30:00 UTC);
///
/// // Without seconds (defaults to 0)
/// let dt = datetime!(2024-1-15 14:30 UTC);
///
/// // Date only (midnight)
/// let dt = datetime!(2024-1-15 UTC);
/// ```
///
/// # Example
/// ```
/// use dukascopy_fx::datetime;
///
/// let timestamp = datetime!(2024-6-15 10:30 UTC);
/// assert_eq!(timestamp.to_string(), "2024-06-15 10:30:00 UTC");
/// ```
#[macro_export]
macro_rules! datetime {
    // Full format: 2024-1-15 14:30:00 UTC
    ($year:literal-$month:literal-$day:literal $hour:literal:$min:literal:$sec:literal UTC) => {
        $crate::time::datetime_utc($year, $month, $day, $hour, $min, $sec)
    };
    // Without seconds: 2024-1-15 14:30 UTC
    ($year:literal-$month:literal-$day:literal $hour:literal:$min:literal UTC) => {
        $crate::time::datetime_utc($year, $month, $day, $hour, $min, 0)
    };
    // Date only: 2024-1-15 UTC
    ($year:literal-$month:literal-$day:literal UTC) => {
        $crate::time::datetime_utc($year, $month, $day, 0, 0, 0)
    };
}

/// Creates a UTC datetime and returns `Option<DateTime<Utc>>` instead of panicking.
///
/// # Example
/// ```
/// use dukascopy_fx::try_datetime;
///
/// let maybe = try_datetime!(2024-6-15 10:30 UTC);
/// assert!(maybe.is_some());
/// ```
#[macro_export]
macro_rules! try_datetime {
    ($year:literal-$month:literal-$day:literal $hour:literal:$min:literal:$sec:literal UTC) => {
        $crate::time::datetime($year, $month, $day, $hour, $min, $sec)
    };
    ($year:literal-$month:literal-$day:literal $hour:literal:$min:literal UTC) => {
        $crate::time::datetime($year, $month, $day, $hour, $min, 0)
    };
    ($year:literal-$month:literal-$day:literal UTC) => {
        $crate::time::datetime($year, $month, $day, 0, 0, 0)
    };
}

/// Creates a ticker with a concise syntax.
///
/// # Example
/// ```
/// use dukascopy_fx::ticker;
///
/// let eur_usd = ticker!("EUR/USD");
/// let gold = ticker!("XAU", "USD");
/// ```
#[macro_export]
macro_rules! ticker {
    // From string: ticker!("EUR/USD")
    ($pair:literal) => {
        $crate::Ticker::parse($pair).expect("Invalid currency pair")
    };
    // From two codes: ticker!("EUR", "USD")
    ($from:literal, $to:literal) => {
        $crate::Ticker::new($from, $to)
    };
}

/// Creates a ticker and returns `Result<Ticker, DukascopyError>`.
///
/// # Example
/// ```
/// use dukascopy_fx::try_ticker;
///
/// let eur_usd = try_ticker!("EUR/USD").unwrap();
/// let gold = try_ticker!("XAU", "USD").unwrap();
/// ```
#[macro_export]
macro_rules! try_ticker {
    ($pair:literal) => {
        $crate::Ticker::parse($pair)
    };
    ($from:literal, $to:literal) => {
        $crate::Ticker::try_new($from, $to)
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_datetime_macro_full() {
        let dt = datetime!(2024-1-15 14:30:45 UTC);
        use chrono::{Datelike, Timelike};
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 14);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 45);
    }

    #[test]
    fn test_datetime_macro_no_seconds() {
        let dt = datetime!(2024-6-15 10:30 UTC);
        use chrono::{Datelike, Timelike};
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 6);
        assert_eq!(dt.hour(), 10);
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 0);
    }

    #[test]
    fn test_datetime_macro_date_only() {
        let dt = datetime!(2024-12-25 UTC);
        use chrono::{Datelike, Timelike};
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 12);
        assert_eq!(dt.day(), 25);
        assert_eq!(dt.hour(), 0);
    }

    #[test]
    fn test_ticker_macro_string() {
        let ticker = ticker!("EUR/USD");
        assert_eq!(ticker.symbol(), "EURUSD");
    }

    #[test]
    fn test_ticker_macro_two_codes() {
        let ticker = ticker!("GBP", "JPY");
        assert_eq!(ticker.symbol(), "GBPJPY");
    }

    #[test]
    fn test_try_datetime_macro_returns_option() {
        let ok = try_datetime!(2024-1-15 14:30 UTC);
        assert!(ok.is_some());
    }

    #[test]
    fn test_try_ticker_macro_returns_result() {
        let ticker = try_ticker!("EUR/USD").unwrap();
        assert_eq!(ticker.symbol(), "EURUSD");

        let err = try_ticker!("BAD$", "USD").unwrap_err();
        assert!(matches!(
            err,
            crate::DukascopyError::InvalidCurrencyCode { code, .. } if code == "BAD$"
        ));
    }
}
