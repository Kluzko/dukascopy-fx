//! Data models for currency pairs and exchange rates.

use crate::core::instrument::{
    resolve_instrument_config, CurrencyCategory, HasInstrumentConfig, InstrumentConfig,
};
use crate::error::DukascopyError;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Represents a currency pair (e.g., USD/PLN).
///
/// # Examples
///
/// ```
/// use dukascopy_fx::CurrencyPair;
///
/// // Using constructor
/// let pair = CurrencyPair::new("USD", "PLN");
///
/// // Using FromStr
/// let pair: CurrencyPair = "EUR/USD".parse().unwrap();
///
/// // Display
/// assert_eq!(format!("{}", pair), "EUR/USD");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CurrencyPair {
    from: String,
    to: String,
}

/// Unified request for rate queries.
///
/// Use [`RateRequest::Pair`] for explicit pair requests (e.g. `EUR/USD`)
/// and [`RateRequest::Symbol`] for single-instrument requests (e.g. `AAPL`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RateRequest {
    Pair(CurrencyPair),
    Symbol(String),
}

impl RateRequest {
    /// Creates a pair request.
    pub fn pair(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::Pair(CurrencyPair::new(from, to))
    }

    /// Creates a symbol request with validation.
    pub fn symbol(symbol: impl Into<String>) -> Result<Self, DukascopyError> {
        let normalized = symbol.into().trim().to_ascii_uppercase();
        CurrencyPair::validate_currency_code(&normalized)?;
        Ok(Self::Symbol(normalized))
    }

    /// Returns pair if this is a pair request.
    pub fn as_pair(&self) -> Option<&CurrencyPair> {
        match self {
            Self::Pair(pair) => Some(pair),
            Self::Symbol(_) => None,
        }
    }

    /// Returns symbol if this is a symbol request.
    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Self::Pair(_) => None,
            Self::Symbol(symbol) => Some(symbol),
        }
    }
}

impl fmt::Display for RateRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pair(pair) => write!(f, "{}", pair),
            Self::Symbol(symbol) => write!(f, "{}", symbol),
        }
    }
}

impl From<CurrencyPair> for RateRequest {
    fn from(value: CurrencyPair) -> Self {
        Self::Pair(value)
    }
}

impl FromStr for RateRequest {
    type Err = DukascopyError;

    /// Parses a request from input string.
    ///
    /// Rules:
    /// - input containing `/` is parsed as explicit pair, e.g. `EUR/USD`
    /// - 6-letter FX shorthand (e.g. `EURUSD`, `XAUUSD`) is parsed as pair
    /// - otherwise input is parsed as symbol, e.g. `AAPL`, `USA500IDX`
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.trim();
        if normalized.is_empty() {
            return Err(DukascopyError::InvalidRequest(
                "Request cannot be empty".to_string(),
            ));
        }

        if normalized.contains('/') {
            return Ok(Self::Pair(CurrencyPair::from_str(normalized)?));
        }

        if is_likely_forex_pair_shorthand(normalized) {
            return Ok(Self::Pair(CurrencyPair::try_new(
                &normalized[0..3],
                &normalized[3..6],
            )?));
        }

        Self::symbol(normalized)
    }
}

fn is_likely_forex_pair_shorthand(input: &str) -> bool {
    let normalized = input.trim().to_ascii_uppercase();
    if normalized.len() != 6 || !normalized.chars().all(|ch| ch.is_ascii_alphabetic()) {
        return false;
    }

    let from = &normalized[0..3];
    let to = &normalized[3..6];

    !matches!(CurrencyCategory::from_code(from), CurrencyCategory::Unknown)
        && !matches!(CurrencyCategory::from_code(to), CurrencyCategory::Unknown)
}

impl CurrencyPair {
    /// Creates a new currency pair.
    ///
    /// # Arguments
    /// * `from` - Source currency code (e.g., "USD")
    /// * `to` - Target currency code (e.g., "PLN")
    ///
    /// # Examples
    /// ```
    /// use dukascopy_fx::CurrencyPair;
    /// let pair = CurrencyPair::new("EUR", "USD");
    /// ```
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: from.into().to_ascii_uppercase(),
            to: to.into().to_ascii_uppercase(),
        }
    }

    /// Creates a currency pair with validation.
    ///
    /// # Arguments
    /// * `from` - Source instrument code
    /// * `to` - Target instrument code
    ///
    /// # Returns
    /// `Ok(CurrencyPair)` if valid, `Err(InvalidCurrencyCode)` otherwise
    ///
    /// # Examples
    /// ```
    /// use dukascopy_fx::CurrencyPair;
    ///
    /// let valid = CurrencyPair::try_new("USD", "EUR");
    /// assert!(valid.is_ok());
    ///
    /// let invalid = CurrencyPair::try_new("U", "EUR");
    /// assert!(invalid.is_err());
    /// ```
    pub fn try_new(from: impl Into<String>, to: impl Into<String>) -> Result<Self, DukascopyError> {
        let from_str = from.into();
        let to_str = to.into();

        Self::validate_currency_code(&from_str)?;
        Self::validate_currency_code(&to_str)?;

        Ok(Self {
            from: from_str.to_ascii_uppercase(),
            to: to_str.to_ascii_uppercase(),
        })
    }

    /// Validates an instrument code.
    fn validate_currency_code(code: &str) -> Result<(), DukascopyError> {
        if code.len() < 2 || code.len() > 12 {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: code.to_string(),
                reason: "Instrument code must be between 2 and 12 characters".to_string(),
            });
        }
        if !code.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: code.to_string(),
                reason: "Instrument code must contain only letters or digits".to_string(),
            });
        }
        Ok(())
    }

    /// Returns the source currency code.
    #[inline]
    pub fn from(&self) -> &str {
        &self.from
    }

    /// Returns the target currency code.
    #[inline]
    pub fn to(&self) -> &str {
        &self.to
    }

    /// Returns the pair as a combined string (e.g., "EURUSD").
    #[inline]
    pub fn as_symbol(&self) -> String {
        format!("{}{}", self.from, self.to)
    }

    /// Returns the inverse pair (e.g., USD/EUR -> EUR/USD).
    pub fn inverse(&self) -> Self {
        Self {
            from: self.to.clone(),
            to: self.from.clone(),
        }
    }

    // ==================== Common Forex Pairs ====================

    /// EUR/USD - Euro / US Dollar
    pub fn eur_usd() -> Self {
        Self::new("EUR", "USD")
    }

    /// GBP/USD - British Pound / US Dollar
    pub fn gbp_usd() -> Self {
        Self::new("GBP", "USD")
    }

    /// USD/JPY - US Dollar / Japanese Yen
    pub fn usd_jpy() -> Self {
        Self::new("USD", "JPY")
    }

    /// USD/CHF - US Dollar / Swiss Franc
    pub fn usd_chf() -> Self {
        Self::new("USD", "CHF")
    }

    /// AUD/USD - Australian Dollar / US Dollar
    pub fn aud_usd() -> Self {
        Self::new("AUD", "USD")
    }

    /// USD/CAD - US Dollar / Canadian Dollar
    pub fn usd_cad() -> Self {
        Self::new("USD", "CAD")
    }

    /// NZD/USD - New Zealand Dollar / US Dollar
    pub fn nzd_usd() -> Self {
        Self::new("NZD", "USD")
    }

    /// XAU/USD - Gold / US Dollar
    pub fn xau_usd() -> Self {
        Self::new("XAU", "USD")
    }

    /// XAG/USD - Silver / US Dollar
    pub fn xag_usd() -> Self {
        Self::new("XAG", "USD")
    }
}

impl fmt::Display for CurrencyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.from, self.to)
    }
}

impl FromStr for CurrencyPair {
    type Err = DukascopyError;

    /// Parse a currency pair from string.
    ///
    /// Accepts formats:
    /// - "EUR/USD" (with slash)
    /// - "EURUSD" (6 characters, no separator; forex shorthand)
    ///
    /// # Examples
    /// ```
    /// use dukascopy_fx::CurrencyPair;
    ///
    /// let pair1: CurrencyPair = "EUR/USD".parse().unwrap();
    /// let pair2: CurrencyPair = "EURUSD".parse().unwrap();
    /// assert_eq!(pair1, pair2);
    /// ```
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();

        if s.contains('/') {
            let parts: Vec<&str> = s.split('/').collect();
            if parts.len() != 2 {
                return Err(DukascopyError::InvalidCurrencyCode {
                    code: s.to_string(),
                    reason: "Invalid pair format. Expected 'BASE/QUOTE'".to_string(),
                });
            }
            Self::try_new(parts[0].trim(), parts[1].trim())
        } else if s.len() == 6 {
            Self::try_new(&s[0..3], &s[3..6])
        } else {
            Err(DukascopyError::InvalidCurrencyCode {
                code: s.to_string(),
                reason: "Invalid pair format. Expected 'BASE/QUOTE' or 6-char forex shorthand like 'EURUSD'".to_string(),
            })
        }
    }
}

impl HasInstrumentConfig for CurrencyPair {
    fn instrument_config(&self) -> InstrumentConfig {
        resolve_instrument_config(&self.from, &self.to)
    }
}

/// Represents a currency exchange rate at a specific timestamp.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrencyExchange {
    /// The currency pair
    pub pair: CurrencyPair,
    /// The exchange rate (mid price: average of ask and bid)
    pub rate: Decimal,
    /// Timestamp when the rate was recorded
    pub timestamp: DateTime<Utc>,
    /// Ask price
    pub ask: Decimal,
    /// Bid price
    pub bid: Decimal,
    /// Ask volume
    pub ask_volume: f32,
    /// Bid volume
    pub bid_volume: f32,
}

impl CurrencyExchange {
    /// Calculate the spread (ask - bid)
    #[inline]
    pub fn spread(&self) -> Decimal {
        self.ask - self.bid
    }

    /// Calculate spread in pips based on the instrument configuration
    pub fn spread_pips(&self) -> Decimal {
        let config = self.pair.instrument_config();
        let multiplier = Decimal::from(10u32.pow(config.decimal_places - 1));
        self.spread() * multiplier
    }
}

impl fmt::Display for CurrencyExchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} @ {} (bid: {}, ask: {}) at {}",
            self.pair, self.rate, self.bid, self.ask, self.timestamp
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod currency_pair {
        use super::*;

        #[test]
        fn test_new() {
            let pair = CurrencyPair::new("usd", "pln");
            assert_eq!(pair.from(), "USD");
            assert_eq!(pair.to(), "PLN");
        }

        #[test]
        fn test_try_new_valid() {
            let pair = CurrencyPair::try_new("EUR", "USD").unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_try_new_invalid_length() {
            let result = CurrencyPair::try_new("E", "USD");
            assert!(result.is_err());

            let result = CurrencyPair::try_new("TOO_LONG_INSTRUMENT_CODE", "USD");
            assert!(result.is_err());
        }

        #[test]
        fn test_try_new_invalid_chars() {
            let result = CurrencyPair::try_new("US$", "EUR");
            assert!(result.is_err());
        }

        #[test]
        fn test_try_new_allows_alphanumeric_instrument_codes() {
            let pair = CurrencyPair::try_new("DE40", "USD").unwrap();
            assert_eq!(pair.from(), "DE40");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_display() {
            let pair = CurrencyPair::new("EUR", "USD");
            assert_eq!(format!("{}", pair), "EUR/USD");
        }

        #[test]
        fn test_as_symbol() {
            let pair = CurrencyPair::new("EUR", "USD");
            assert_eq!(pair.as_symbol(), "EURUSD");
        }

        #[test]
        fn test_inverse() {
            let pair = CurrencyPair::new("EUR", "USD");
            let inverse = pair.inverse();
            assert_eq!(inverse.from(), "USD");
            assert_eq!(inverse.to(), "EUR");
        }

        #[test]
        fn test_from_str_with_slash() {
            let pair: CurrencyPair = "EUR/USD".parse().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_from_str_without_slash() {
            let pair: CurrencyPair = "EURUSD".parse().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_from_str_with_whitespace() {
            let pair: CurrencyPair = "  EUR / USD  ".parse().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_from_str_lowercase() {
            let pair: CurrencyPair = "eur/usd".parse().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_from_str_invalid() {
            assert!("EUR".parse::<CurrencyPair>().is_err());
            assert!("EUR/USD/GBP".parse::<CurrencyPair>().is_err());
            assert!("EURUSDD".parse::<CurrencyPair>().is_err());
        }

        #[test]
        fn test_from_str_with_non_fx_codes() {
            let pair: CurrencyPair = "DE40/USD".parse().unwrap();
            assert_eq!(pair.from(), "DE40");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_common_pairs() {
            assert_eq!(CurrencyPair::eur_usd().as_symbol(), "EURUSD");
            assert_eq!(CurrencyPair::gbp_usd().as_symbol(), "GBPUSD");
            assert_eq!(CurrencyPair::usd_jpy().as_symbol(), "USDJPY");
            assert_eq!(CurrencyPair::xau_usd().as_symbol(), "XAUUSD");
        }

        #[test]
        fn test_equality() {
            let pair1 = CurrencyPair::new("EUR", "USD");
            let pair2 = CurrencyPair::new("eur", "usd");
            assert_eq!(pair1, pair2);
        }

        #[test]
        fn test_hash() {
            use std::collections::HashSet;
            let mut set = HashSet::new();
            set.insert(CurrencyPair::new("EUR", "USD"));
            assert!(set.contains(&CurrencyPair::new("eur", "usd")));
        }

        #[test]
        fn test_instrument_config() {
            let standard = CurrencyPair::new("EUR", "USD");
            assert_eq!(standard.price_divisor(), 100_000.0);

            let jpy = CurrencyPair::new("USD", "JPY");
            assert_eq!(jpy.price_divisor(), 1_000.0);

            let gold = CurrencyPair::new("XAU", "USD");
            assert_eq!(gold.price_divisor(), 1_000.0);
        }
    }

    mod rate_request {
        use super::*;

        #[test]
        fn test_parse_pair_request() {
            let request: RateRequest = "EUR/USD".parse().unwrap();
            let pair = request.as_pair().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_parse_pair_request_with_whitespace() {
            let request: RateRequest = "  eur / usd  ".parse().unwrap();
            let pair = request.as_pair().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_parse_symbol_request() {
            let request: RateRequest = "aapl".parse().unwrap();
            assert_eq!(request.as_symbol(), Some("AAPL"));
        }

        #[test]
        fn test_parse_forex_shorthand_without_slash() {
            let request: RateRequest = "eurusd".parse().unwrap();
            let pair = request.as_pair().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_parse_non_fx_six_char_code_as_symbol() {
            let request: RateRequest = "aaplus".parse().unwrap();
            assert_eq!(request.as_symbol(), Some("AAPLUS"));
        }

        #[test]
        fn test_symbol_constructor_validation() {
            assert!(RateRequest::symbol("AAPL").is_ok());
            assert!(RateRequest::symbol("X").is_err());
            assert!(RateRequest::symbol("BAD$").is_err());
        }

        #[test]
        fn test_pair_constructor_normalizes_codes() {
            let request = RateRequest::pair("eur", "usd");
            let pair = request.as_pair().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_pair_variant_as_symbol_is_none() {
            let request = RateRequest::pair("EUR", "USD");
            assert_eq!(request.as_symbol(), None);
        }

        #[test]
        fn test_symbol_variant_as_pair_is_none() {
            let request = RateRequest::symbol("AAPL").unwrap();
            assert_eq!(request.as_pair(), None);
        }

        #[test]
        fn test_display_for_pair_and_symbol() {
            let pair_request = RateRequest::pair("eur", "usd");
            let symbol_request = RateRequest::symbol("msft").unwrap();

            assert_eq!(pair_request.to_string(), "EUR/USD");
            assert_eq!(symbol_request.to_string(), "MSFT");
        }

        #[test]
        fn test_from_currency_pair_conversion() {
            let pair = CurrencyPair::new("GBP", "JPY");
            let request: RateRequest = pair.clone().into();
            assert_eq!(request.as_pair(), Some(&pair));
        }

        #[test]
        fn test_parse_empty_request() {
            let err = "   ".parse::<RateRequest>().unwrap_err();
            assert!(matches!(err, DukascopyError::InvalidRequest(_)));
        }

        #[test]
        fn test_parse_invalid_pair_request_propagates_validation_error() {
            let err = "EUR/US$".parse::<RateRequest>().unwrap_err();
            assert!(matches!(
                err,
                DukascopyError::InvalidCurrencyCode { code, .. } if code == "US$"
            ));
        }
    }

    mod currency_exchange {
        use super::*;
        use rust_decimal::Decimal;
        use std::str::FromStr;

        #[test]
        fn test_spread() {
            let exchange = CurrencyExchange {
                pair: CurrencyPair::new("EUR", "USD"),
                rate: Decimal::from_str("1.10450").unwrap(),
                timestamp: Utc::now(),
                ask: Decimal::from_str("1.10500").unwrap(),
                bid: Decimal::from_str("1.10400").unwrap(),
                ask_volume: 1.0,
                bid_volume: 1.0,
            };
            assert_eq!(exchange.spread(), Decimal::from_str("0.00100").unwrap());
        }

        #[test]
        fn test_display() {
            let exchange = CurrencyExchange {
                pair: CurrencyPair::new("EUR", "USD"),
                rate: Decimal::from_str("1.10450").unwrap(),
                timestamp: Utc::now(),
                ask: Decimal::from_str("1.10500").unwrap(),
                bid: Decimal::from_str("1.10400").unwrap(),
                ask_volume: 1.0,
                bid_volume: 1.0,
            };
            let display = format!("{}", exchange);
            assert!(display.contains("EUR/USD"));
            assert!(display.contains("1.10450"));
        }
    }
}
