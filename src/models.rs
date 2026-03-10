//! Data models for currency pairs and exchange rates.

use crate::core::instrument::{resolve_instrument_config, HasInstrumentConfig, InstrumentConfig};
use crate::error::DukascopyError;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
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
    from: SmolStr,
    to: SmolStr,
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

/// Parsing strategy for request input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RequestParseMode {
    /// Best-effort parsing: slash pairs first, then known FX shorthand, otherwise symbol.
    #[default]
    Auto,
    /// Require pair parsing semantics.
    PairOnly,
    /// Require symbol parsing semantics.
    SymbolOnly,
}

impl RateRequest {
    /// Creates a pair request.
    pub fn pair(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self::Pair(CurrencyPair::new(from, to))
    }

    /// Creates a symbol request with validation.
    pub fn symbol(symbol: impl Into<String>) -> Result<Self, DukascopyError> {
        let raw = symbol.into();
        Self::symbol_from_trimmed(raw.trim())
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

    /// Parses a request with an explicit parse mode.
    pub fn parse_with_mode(input: &str, mode: RequestParseMode) -> Result<Self, DukascopyError> {
        let normalized = input.trim();
        if normalized.is_empty() {
            return Err(DukascopyError::InvalidRequest(
                "Request cannot be empty".to_string(),
            ));
        }

        match mode {
            RequestParseMode::Auto => {
                if let Some(pair) = parse_compact_pair_slash(normalized) {
                    return Ok(Self::Pair(pair));
                }

                if normalized.as_bytes().contains(&b'/') {
                    return Ok(Self::Pair(parse_pair_with_slash(normalized)?));
                }

                if let Some((from, to)) = split_known_fx_pair_shorthand(normalized) {
                    // Parsed shorthand is guaranteed to be 6 ASCII letters, so this is a safe
                    // normalization-only construction path.
                    return Ok(Self::Pair(CurrencyPair::new(from, to)));
                }

                Self::symbol_from_trimmed(normalized)
            }
            RequestParseMode::PairOnly => {
                if let Some(pair) = parse_compact_pair_slash(normalized) {
                    return Ok(Self::Pair(pair));
                }

                if normalized.as_bytes().contains(&b'/') {
                    return Ok(Self::Pair(parse_pair_with_slash(normalized)?));
                }

                if let Some((from, to)) = split_ascii_pair_shorthand(normalized) {
                    return Ok(Self::Pair(CurrencyPair::new(from, to)));
                }

                Err(DukascopyError::InvalidRequest(format!(
                    "PairOnly parsing expected 'BASE/QUOTE' or 6-letter pair shorthand, got '{}'",
                    normalized
                )))
            }
            RequestParseMode::SymbolOnly => Self::symbol_from_trimmed(normalized),
        }
    }

    #[inline]
    fn symbol_from_trimmed(trimmed: &str) -> Result<Self, DukascopyError> {
        let normalized = normalize_code_checked(trimmed.to_string()).map_err(|err| match err {
            DukascopyError::InvalidCurrencyCode { reason, .. } => {
                DukascopyError::InvalidCurrencyCode {
                    code: trimmed.to_ascii_uppercase(),
                    reason,
                }
            }
            other => other,
        })?;
        Ok(Self::Symbol(normalized))
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
        Self::parse_with_mode(input, RequestParseMode::Auto)
    }
}

#[inline]
fn split_known_fx_pair_shorthand(input: &str) -> Option<(&str, &str)> {
    let (from, to) = split_ascii_pair_shorthand(input)?;

    if is_known_fx_code_case_insensitive(from.as_bytes())
        && is_known_fx_code_case_insensitive(to.as_bytes())
    {
        Some((from, to))
    } else {
        None
    }
}

#[inline]
fn split_ascii_pair_shorthand(input: &str) -> Option<(&str, &str)> {
    let bytes = input.as_bytes();
    if bytes.len() != 6 {
        return None;
    }

    if !bytes[0].is_ascii_alphabetic()
        || !bytes[1].is_ascii_alphabetic()
        || !bytes[2].is_ascii_alphabetic()
        || !bytes[3].is_ascii_alphabetic()
        || !bytes[4].is_ascii_alphabetic()
        || !bytes[5].is_ascii_alphabetic()
    {
        return None;
    }

    Some((&input[0..3], &input[3..6]))
}

#[inline]
fn is_known_fx_code_case_insensitive(code: &[u8]) -> bool {
    let Some(value) = code3_ascii_upper(code) else {
        return false;
    };

    matches!(
        value,
        CODE_JPY
            | CODE_RUB
            | CODE_XAU
            | CODE_XAG
            | CODE_XPT
            | CODE_XPD
            | CODE_USD
            | CODE_EUR
            | CODE_GBP
            | CODE_AUD
            | CODE_NZD
            | CODE_CAD
            | CODE_CHF
            | CODE_SEK
            | CODE_NOK
            | CODE_DKK
            | CODE_SGD
            | CODE_HKD
            | CODE_MXN
            | CODE_ZAR
            | CODE_TRY
            | CODE_PLN
            | CODE_CZK
            | CODE_HUF
            | CODE_CNH
            | CODE_CNY
            | CODE_INR
            | CODE_THB
            | CODE_KRW
            | CODE_TWD
            | CODE_BRL
            | CODE_ILS
    )
}

#[inline]
fn parse_compact_pair_slash(input: &str) -> Option<CurrencyPair> {
    let bytes = input.as_bytes();
    if bytes.len() != 7 || bytes[3] != b'/' {
        return None;
    }

    if !is_ascii_alphanumeric3(&bytes[0..3]) || !is_ascii_alphanumeric3(&bytes[4..7]) {
        return None;
    }

    Some(CurrencyPair::new(&input[0..3], &input[4..7]))
}

#[inline]
fn parse_pair_with_slash(input: &str) -> Result<CurrencyPair, DukascopyError> {
    let Some((from_raw, to_raw)) = input.split_once('/') else {
        return Err(DukascopyError::InvalidCurrencyCode {
            code: input.to_string(),
            reason: "Invalid pair format. Expected 'BASE/QUOTE'".to_string(),
        });
    };

    if to_raw.as_bytes().contains(&b'/') {
        return Err(DukascopyError::InvalidCurrencyCode {
            code: input.to_string(),
            reason: "Invalid pair format. Expected 'BASE/QUOTE'".to_string(),
        });
    }

    CurrencyPair::try_new(from_raw.trim(), to_raw.trim())
}

#[inline]
fn is_ascii_alphanumeric3(bytes: &[u8]) -> bool {
    bytes[0].is_ascii_alphanumeric()
        && bytes[1].is_ascii_alphanumeric()
        && bytes[2].is_ascii_alphanumeric()
}

#[inline]
fn normalize_code_checked(mut code: String) -> Result<String, DukascopyError> {
    let len = code.len();
    if !(2..=12).contains(&len) {
        return Err(DukascopyError::InvalidCurrencyCode {
            code,
            reason: "Instrument code must be between 2 and 12 characters".to_string(),
        });
    }

    let mut has_lowercase = false;
    for &b in code.as_bytes() {
        if !b.is_ascii_alphanumeric() {
            return Err(DukascopyError::InvalidCurrencyCode {
                code,
                reason: "Instrument code must contain only letters or digits".to_string(),
            });
        }
        has_lowercase |= b.is_ascii_lowercase();
    }

    if has_lowercase {
        code.make_ascii_uppercase();
    }

    Ok(code)
}

#[inline]
fn normalize_code_checked_smol(code: &str) -> Result<SmolStr, DukascopyError> {
    let len = code.len();
    if !(2..=12).contains(&len) {
        return Err(DukascopyError::InvalidCurrencyCode {
            code: code.to_string(),
            reason: "Instrument code must be between 2 and 12 characters".to_string(),
        });
    }

    let bytes = code.as_bytes();
    let mut has_lowercase = false;
    for &b in bytes {
        if !b.is_ascii_alphanumeric() {
            return Err(DukascopyError::InvalidCurrencyCode {
                code: code.to_string(),
                reason: "Instrument code must contain only letters or digits".to_string(),
            });
        }
        has_lowercase |= b.is_ascii_lowercase();
    }

    if has_lowercase {
        return Ok(SmolStr::new(code.to_ascii_uppercase()));
    }

    Ok(SmolStr::new(code))
}

#[inline]
fn normalize_ascii_upper(code: &str) -> SmolStr {
    if code.as_bytes().iter().any(|b| b.is_ascii_lowercase()) {
        return SmolStr::new(code.to_ascii_uppercase());
    }
    SmolStr::new(code)
}

#[inline]
const fn code3(a: u8, b: u8, c: u8) -> u32 {
    ((a as u32) << 16) | ((b as u32) << 8) | (c as u32)
}

#[inline]
fn code3_ascii_upper(code: &[u8]) -> Option<u32> {
    if code.len() != 3 {
        return None;
    }

    Some(code3(
        code[0].to_ascii_uppercase(),
        code[1].to_ascii_uppercase(),
        code[2].to_ascii_uppercase(),
    ))
}

const CODE_JPY: u32 = code3(b'J', b'P', b'Y');
const CODE_RUB: u32 = code3(b'R', b'U', b'B');
const CODE_XAU: u32 = code3(b'X', b'A', b'U');
const CODE_XAG: u32 = code3(b'X', b'A', b'G');
const CODE_XPT: u32 = code3(b'X', b'P', b'T');
const CODE_XPD: u32 = code3(b'X', b'P', b'D');
const CODE_USD: u32 = code3(b'U', b'S', b'D');
const CODE_EUR: u32 = code3(b'E', b'U', b'R');
const CODE_GBP: u32 = code3(b'G', b'B', b'P');
const CODE_AUD: u32 = code3(b'A', b'U', b'D');
const CODE_NZD: u32 = code3(b'N', b'Z', b'D');
const CODE_CAD: u32 = code3(b'C', b'A', b'D');
const CODE_CHF: u32 = code3(b'C', b'H', b'F');
const CODE_SEK: u32 = code3(b'S', b'E', b'K');
const CODE_NOK: u32 = code3(b'N', b'O', b'K');
const CODE_DKK: u32 = code3(b'D', b'K', b'K');
const CODE_SGD: u32 = code3(b'S', b'G', b'D');
const CODE_HKD: u32 = code3(b'H', b'K', b'D');
const CODE_MXN: u32 = code3(b'M', b'X', b'N');
const CODE_ZAR: u32 = code3(b'Z', b'A', b'R');
const CODE_TRY: u32 = code3(b'T', b'R', b'Y');
const CODE_PLN: u32 = code3(b'P', b'L', b'N');
const CODE_CZK: u32 = code3(b'C', b'Z', b'K');
const CODE_HUF: u32 = code3(b'H', b'U', b'F');
const CODE_CNH: u32 = code3(b'C', b'N', b'H');
const CODE_CNY: u32 = code3(b'C', b'N', b'Y');
const CODE_INR: u32 = code3(b'I', b'N', b'R');
const CODE_THB: u32 = code3(b'T', b'H', b'B');
const CODE_KRW: u32 = code3(b'K', b'R', b'W');
const CODE_TWD: u32 = code3(b'T', b'W', b'D');
const CODE_BRL: u32 = code3(b'B', b'R', b'L');
const CODE_ILS: u32 = code3(b'I', b'L', b'S');

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
        let from = from.into();
        let to = to.into();
        Self {
            from: normalize_ascii_upper(&from),
            to: normalize_ascii_upper(&to),
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
        let from = from.into();
        let to = to.into();
        let from_norm = normalize_code_checked_smol(&from)?;
        let to_norm = normalize_code_checked_smol(&to)?;

        Ok(Self {
            from: from_norm,
            to: to_norm,
        })
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
        let mut symbol = String::with_capacity(self.from.len() + self.to.len());
        symbol.push_str(&self.from);
        symbol.push_str(&self.to);
        symbol
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

        if s.len() == 6 && !s.as_bytes().contains(&b'/') {
            Self::try_new(&s[0..3], &s[3..6])
        } else if s.as_bytes().contains(&b'/') {
            parse_pair_with_slash(s)
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
        fn test_parse_with_mode_pair_only() {
            let request =
                RateRequest::parse_with_mode("EURUSD", RequestParseMode::PairOnly).unwrap();
            let pair = request.as_pair().unwrap();
            assert_eq!(pair.from(), "EUR");
            assert_eq!(pair.to(), "USD");
        }

        #[test]
        fn test_parse_with_mode_pair_only_rejects_symbol() {
            let err = RateRequest::parse_with_mode("AAPL", RequestParseMode::PairOnly).unwrap_err();
            assert!(matches!(err, DukascopyError::InvalidRequest(_)));
        }

        #[test]
        fn test_parse_with_mode_symbol_only() {
            let request =
                RateRequest::parse_with_mode("aapl", RequestParseMode::SymbolOnly).unwrap();
            assert_eq!(request.as_symbol(), Some("AAPL"));
        }

        #[test]
        fn test_parse_with_mode_symbol_only_rejects_pair_format() {
            let err =
                RateRequest::parse_with_mode("EUR/USD", RequestParseMode::SymbolOnly).unwrap_err();
            assert!(matches!(
                err,
                DukascopyError::InvalidCurrencyCode { code, .. } if code == "EUR/USD"
            ));
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
