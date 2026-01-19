//! Instrument configuration for price scaling and decimal precision.
//!
//! This module provides a standardized, extensible way to handle different instrument types
//! (forex pairs, metals, indices, etc.) with their specific price divisors.
//!
//! # Price Divisor Rules (Verified from multiple sources)
//!
//! | Instrument Type | Divisor | Decimal Places | Examples |
//! |-----------------|---------|----------------|----------|
//! | Standard Forex  | 100,000 | 5              | EUR/USD, GBP/USD |
//! | JPY Pairs       | 1,000   | 3              | USD/JPY, EUR/JPY |
//! | Metals          | 1,000   | 3              | XAU/USD, XAG/USD |
//! | RUB Pairs       | 1,000   | 3              | USD/RUB |
//!
//! Sources:
//! - <https://github.com/giuse88/duka>
//! - <https://github.com/svaningelgem/spark_bi5_datasource>
//! - <https://www.dukascopy.com/wiki/en/development/strategy-api/instruments/>

use std::borrow::Cow;

/// Price divisor for standard currency pairs (5 decimal places)
pub const DIVISOR_5_DECIMALS: f64 = 100_000.0;

/// Price divisor for 3 decimal place instruments (JPY, metals, RUB)
pub const DIVISOR_3_DECIMALS: f64 = 1_000.0;

/// Price divisor for 2 decimal place instruments (some indices)
pub const DIVISOR_2_DECIMALS: f64 = 100.0;

/// Configuration for an instrument's price scaling
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InstrumentConfig {
    /// Divisor to convert raw tick price to actual price
    pub price_divisor: f64,
    /// Number of decimal places for the instrument
    pub decimal_places: u32,
}

impl InstrumentConfig {
    /// Creates a new instrument configuration
    #[inline]
    pub const fn new(price_divisor: f64, decimal_places: u32) -> Self {
        Self {
            price_divisor,
            decimal_places,
        }
    }

    /// Standard forex pair configuration (5 decimal places, divisor 100,000)
    /// Used for: EUR/USD, GBP/USD, AUD/USD, NZD/USD, USD/CAD, USD/CHF, EUR/GBP, etc.
    pub const STANDARD: Self = Self::new(DIVISOR_5_DECIMALS, 5);

    /// JPY forex pair configuration (3 decimal places, divisor 1,000)
    /// Used for: USD/JPY, EUR/JPY, GBP/JPY, AUD/JPY, etc.
    pub const JPY: Self = Self::new(DIVISOR_3_DECIMALS, 3);

    /// Metals configuration (3 decimal places, divisor 1,000)
    /// Used for: XAU/USD, XAG/USD, XAU/EUR, XAG/EUR
    pub const METALS: Self = Self::new(DIVISOR_3_DECIMALS, 3);

    /// RUB pairs configuration (3 decimal places, divisor 1,000)
    /// Used for: USD/RUB, EUR/RUB
    pub const RUB: Self = Self::new(DIVISOR_3_DECIMALS, 3);

    /// Index configuration (2 decimal places, divisor 100)
    /// Used for: Various index CFDs
    pub const INDEX: Self = Self::new(DIVISOR_2_DECIMALS, 2);
}

impl Default for InstrumentConfig {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Categories of currencies/instruments that require special handling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencyCategory {
    /// Standard 5-decimal currencies (USD, EUR, GBP, AUD, NZD, CAD, CHF, etc.)
    Standard,
    /// Japanese Yen - uses 3 decimal places
    Jpy,
    /// Russian Ruble - uses 3 decimal places
    Rub,
    /// Precious metals (XAU, XAG, XPT, XPD)
    Metal,
    /// Unknown category - falls back to standard
    Unknown,
}

impl CurrencyCategory {
    /// Categorizes a currency code
    ///
    /// # Arguments
    /// * `code` - The 3-letter currency code (case-insensitive)
    ///
    /// # Returns
    /// The category of the currency
    pub fn from_code(code: &str) -> Self {
        let code_upper: Cow<str> = if code.chars().all(|c| c.is_ascii_uppercase()) {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(code.to_ascii_uppercase())
        };

        match code_upper.as_ref() {
            "JPY" => Self::Jpy,
            "RUB" => Self::Rub,
            "XAU" | "XAG" | "XPT" | "XPD" => Self::Metal,
            // Standard currencies - explicitly listed for clarity
            "USD" | "EUR" | "GBP" | "AUD" | "NZD" | "CAD" | "CHF" | "SEK" | "NOK" | "DKK"
            | "SGD" | "HKD" | "MXN" | "ZAR" | "TRY" | "PLN" | "CZK" | "HUF" | "CNH" | "CNY"
            | "INR" | "THB" | "KRW" | "TWD" | "BRL" | "ILS" => Self::Standard,
            _ => Self::Unknown,
        }
    }

    /// Returns the instrument configuration for this category
    pub const fn config(&self) -> InstrumentConfig {
        match self {
            Self::Jpy => InstrumentConfig::JPY,
            Self::Rub => InstrumentConfig::RUB,
            Self::Metal => InstrumentConfig::METALS,
            Self::Standard | Self::Unknown => InstrumentConfig::STANDARD,
        }
    }
}

/// Resolves the instrument configuration for a currency pair.
///
/// The resolution follows these rules:
/// 1. If either currency is a metal (XAU, XAG, XPT, XPD) → Metal config (divisor 1,000)
/// 2. If either currency is JPY → JPY config (divisor 1,000)
/// 3. If either currency is RUB → RUB config (divisor 1,000)
/// 4. Otherwise → Standard config (divisor 100,000)
///
/// # Arguments
/// * `from` - Source currency code (e.g., "USD")
/// * `to` - Target currency code (e.g., "JPY")
///
/// # Returns
/// The appropriate `InstrumentConfig` for the pair
///
/// # Examples
/// ```
/// use dukascopy_fx::instrument::resolve_instrument_config;
///
/// // Standard pair
/// let config = resolve_instrument_config("EUR", "USD");
/// assert_eq!(config.price_divisor, 100_000.0);
///
/// // JPY pair
/// let config = resolve_instrument_config("USD", "JPY");
/// assert_eq!(config.price_divisor, 1_000.0);
///
/// // Gold
/// let config = resolve_instrument_config("XAU", "USD");
/// assert_eq!(config.price_divisor, 1_000.0);
/// ```
pub fn resolve_instrument_config(from: &str, to: &str) -> InstrumentConfig {
    let from_cat = CurrencyCategory::from_code(from);
    let to_cat = CurrencyCategory::from_code(to);

    // Priority order: Metals > JPY > RUB > Standard
    // This handles cases like XAU/JPY correctly (uses metal config)
    match (from_cat, to_cat) {
        // Metals take priority
        (CurrencyCategory::Metal, _) | (_, CurrencyCategory::Metal) => InstrumentConfig::METALS,
        // Then JPY
        (CurrencyCategory::Jpy, _) | (_, CurrencyCategory::Jpy) => InstrumentConfig::JPY,
        // Then RUB
        (CurrencyCategory::Rub, _) | (_, CurrencyCategory::Rub) => InstrumentConfig::RUB,
        // Default to standard
        _ => InstrumentConfig::STANDARD,
    }
}

/// Trait for types that can provide instrument configuration
pub trait HasInstrumentConfig {
    /// Returns the instrument configuration for this type
    fn instrument_config(&self) -> InstrumentConfig;

    /// Returns the price divisor for this instrument
    #[inline]
    fn price_divisor(&self) -> f64 {
        self.instrument_config().price_divisor
    }

    /// Returns the number of decimal places for this instrument
    #[inline]
    fn decimal_places(&self) -> u32 {
        self.instrument_config().decimal_places
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== CurrencyCategory Tests ====================

    mod currency_category {
        use super::*;

        #[test]
        fn test_jpy_detection() {
            assert_eq!(CurrencyCategory::from_code("JPY"), CurrencyCategory::Jpy);
            assert_eq!(CurrencyCategory::from_code("jpy"), CurrencyCategory::Jpy);
            assert_eq!(CurrencyCategory::from_code("Jpy"), CurrencyCategory::Jpy);
        }

        #[test]
        fn test_rub_detection() {
            assert_eq!(CurrencyCategory::from_code("RUB"), CurrencyCategory::Rub);
            assert_eq!(CurrencyCategory::from_code("rub"), CurrencyCategory::Rub);
        }

        #[test]
        fn test_metal_detection() {
            assert_eq!(CurrencyCategory::from_code("XAU"), CurrencyCategory::Metal);
            assert_eq!(CurrencyCategory::from_code("XAG"), CurrencyCategory::Metal);
            assert_eq!(CurrencyCategory::from_code("XPT"), CurrencyCategory::Metal);
            assert_eq!(CurrencyCategory::from_code("XPD"), CurrencyCategory::Metal);
            assert_eq!(CurrencyCategory::from_code("xau"), CurrencyCategory::Metal);
        }

        #[test]
        fn test_standard_currencies() {
            let standard_currencies = [
                "USD", "EUR", "GBP", "AUD", "NZD", "CAD", "CHF", "SEK", "NOK", "DKK", "SGD", "HKD",
                "MXN", "ZAR", "TRY", "PLN", "CZK", "HUF", "CNH", "CNY", "INR", "THB", "BRL", "ILS",
            ];

            for code in standard_currencies {
                assert_eq!(
                    CurrencyCategory::from_code(code),
                    CurrencyCategory::Standard,
                    "Failed for currency: {}",
                    code
                );
            }
        }

        #[test]
        fn test_unknown_currency() {
            assert_eq!(
                CurrencyCategory::from_code("XYZ"),
                CurrencyCategory::Unknown
            );
            assert_eq!(
                CurrencyCategory::from_code("ABC"),
                CurrencyCategory::Unknown
            );
            assert_eq!(
                CurrencyCategory::from_code("FOO"),
                CurrencyCategory::Unknown
            );
        }

        #[test]
        fn test_category_config() {
            assert_eq!(CurrencyCategory::Jpy.config(), InstrumentConfig::JPY);
            assert_eq!(CurrencyCategory::Rub.config(), InstrumentConfig::RUB);
            assert_eq!(CurrencyCategory::Metal.config(), InstrumentConfig::METALS);
            assert_eq!(
                CurrencyCategory::Standard.config(),
                InstrumentConfig::STANDARD
            );
            assert_eq!(
                CurrencyCategory::Unknown.config(),
                InstrumentConfig::STANDARD
            );
        }
    }

    // ==================== resolve_instrument_config Tests ====================

    mod resolve_config {
        use super::*;

        #[test]
        fn test_standard_pairs() {
            let pairs = [
                ("EUR", "USD"),
                ("GBP", "USD"),
                ("AUD", "USD"),
                ("NZD", "USD"),
                ("USD", "CAD"),
                ("USD", "CHF"),
                ("EUR", "GBP"),
                ("EUR", "CHF"),
                ("GBP", "CHF"),
                ("AUD", "NZD"),
                ("USD", "PLN"),
                ("EUR", "PLN"),
            ];

            for (from, to) in pairs {
                let config = resolve_instrument_config(from, to);
                assert_eq!(
                    config,
                    InstrumentConfig::STANDARD,
                    "Failed for pair: {}/{}",
                    from,
                    to
                );
                assert_eq!(config.price_divisor, 100_000.0);
                assert_eq!(config.decimal_places, 5);
            }
        }

        #[test]
        fn test_jpy_pairs() {
            let pairs = [
                ("USD", "JPY"),
                ("EUR", "JPY"),
                ("GBP", "JPY"),
                ("AUD", "JPY"),
                ("NZD", "JPY"),
                ("CAD", "JPY"),
                ("CHF", "JPY"),
            ];

            for (from, to) in pairs {
                let config = resolve_instrument_config(from, to);
                assert_eq!(
                    config,
                    InstrumentConfig::JPY,
                    "Failed for pair: {}/{}",
                    from,
                    to
                );
                assert_eq!(config.price_divisor, 1_000.0);
                assert_eq!(config.decimal_places, 3);
            }
        }

        #[test]
        fn test_jpy_pairs_reverse() {
            // JPY as base currency (less common but should work)
            let config = resolve_instrument_config("JPY", "USD");
            assert_eq!(config, InstrumentConfig::JPY);
        }

        #[test]
        fn test_jpy_case_insensitive() {
            assert_eq!(
                resolve_instrument_config("usd", "jpy"),
                InstrumentConfig::JPY
            );
            assert_eq!(
                resolve_instrument_config("Usd", "Jpy"),
                InstrumentConfig::JPY
            );
            assert_eq!(
                resolve_instrument_config("USD", "jpy"),
                InstrumentConfig::JPY
            );
        }

        #[test]
        fn test_metal_pairs() {
            let pairs = [
                ("XAU", "USD"),
                ("XAG", "USD"),
                ("XAU", "EUR"),
                ("XAG", "EUR"),
                ("XPT", "USD"),
                ("XPD", "USD"),
            ];

            for (from, to) in pairs {
                let config = resolve_instrument_config(from, to);
                assert_eq!(
                    config,
                    InstrumentConfig::METALS,
                    "Failed for pair: {}/{}",
                    from,
                    to
                );
                assert_eq!(config.price_divisor, 1_000.0);
                assert_eq!(config.decimal_places, 3);
            }
        }

        #[test]
        fn test_metal_takes_priority_over_jpy() {
            // XAU/JPY should use metal config, not JPY config
            let config = resolve_instrument_config("XAU", "JPY");
            assert_eq!(config, InstrumentConfig::METALS);
        }

        #[test]
        fn test_rub_pairs() {
            let pairs = [("USD", "RUB"), ("EUR", "RUB")];

            for (from, to) in pairs {
                let config = resolve_instrument_config(from, to);
                assert_eq!(
                    config,
                    InstrumentConfig::RUB,
                    "Failed for pair: {}/{}",
                    from,
                    to
                );
                assert_eq!(config.price_divisor, 1_000.0);
                assert_eq!(config.decimal_places, 3);
            }
        }

        #[test]
        fn test_unknown_currency_defaults_to_standard() {
            let config = resolve_instrument_config("ABC", "XYZ");
            assert_eq!(config, InstrumentConfig::STANDARD);
        }

        #[test]
        fn test_mixed_unknown_and_known() {
            // Unknown + Standard = Standard
            let config = resolve_instrument_config("ABC", "USD");
            assert_eq!(config, InstrumentConfig::STANDARD);

            // Unknown + JPY = JPY
            let config = resolve_instrument_config("ABC", "JPY");
            assert_eq!(config, InstrumentConfig::JPY);
        }
    }

    // ==================== InstrumentConfig Tests ====================

    mod instrument_config {
        use super::*;

        #[test]
        fn test_config_constants() {
            assert_eq!(InstrumentConfig::STANDARD.price_divisor, 100_000.0);
            assert_eq!(InstrumentConfig::STANDARD.decimal_places, 5);

            assert_eq!(InstrumentConfig::JPY.price_divisor, 1_000.0);
            assert_eq!(InstrumentConfig::JPY.decimal_places, 3);

            assert_eq!(InstrumentConfig::METALS.price_divisor, 1_000.0);
            assert_eq!(InstrumentConfig::METALS.decimal_places, 3);

            assert_eq!(InstrumentConfig::RUB.price_divisor, 1_000.0);
            assert_eq!(InstrumentConfig::RUB.decimal_places, 3);

            assert_eq!(InstrumentConfig::INDEX.price_divisor, 100.0);
            assert_eq!(InstrumentConfig::INDEX.decimal_places, 2);
        }

        #[test]
        fn test_default_is_standard() {
            assert_eq!(InstrumentConfig::default(), InstrumentConfig::STANDARD);
        }

        #[test]
        fn test_config_equality() {
            let a = InstrumentConfig::new(100_000.0, 5);
            let b = InstrumentConfig::STANDARD;
            assert_eq!(a, b);
        }

        #[test]
        fn test_config_clone() {
            let original = InstrumentConfig::JPY;
            let cloned = original;
            assert_eq!(original, cloned);
        }
    }

    // ==================== Edge Cases ====================

    mod edge_cases {
        use super::*;

        #[test]
        fn test_empty_string() {
            // Empty strings should return Unknown category
            assert_eq!(CurrencyCategory::from_code(""), CurrencyCategory::Unknown);
        }

        #[test]
        fn test_whitespace() {
            assert_eq!(CurrencyCategory::from_code(" "), CurrencyCategory::Unknown);
            assert_eq!(CurrencyCategory::from_code("  "), CurrencyCategory::Unknown);
        }

        #[test]
        fn test_partial_matches() {
            // "JP" should not match JPY
            assert_eq!(CurrencyCategory::from_code("JP"), CurrencyCategory::Unknown);
            // "JPYY" should not match JPY
            assert_eq!(
                CurrencyCategory::from_code("JPYY"),
                CurrencyCategory::Unknown
            );
        }

        #[test]
        fn test_numeric_input() {
            assert_eq!(
                CurrencyCategory::from_code("123"),
                CurrencyCategory::Unknown
            );
        }

        #[test]
        fn test_special_characters() {
            assert_eq!(
                CurrencyCategory::from_code("US$"),
                CurrencyCategory::Unknown
            );
            assert_eq!(
                CurrencyCategory::from_code("€UR"),
                CurrencyCategory::Unknown
            );
        }

        #[test]
        fn test_same_currency_pair() {
            // USD/USD - unusual but should work
            let config = resolve_instrument_config("USD", "USD");
            assert_eq!(config, InstrumentConfig::STANDARD);

            // JPY/JPY
            let config = resolve_instrument_config("JPY", "JPY");
            assert_eq!(config, InstrumentConfig::JPY);
        }
    }

    // ==================== Real-World Price Conversion Tests ====================

    mod price_conversion {
        use super::*;

        #[test]
        fn test_eurusd_price_conversion() {
            let config = resolve_instrument_config("EUR", "USD");
            let raw_price: u32 = 110500; // Represents 1.10500
            let actual_price = raw_price as f64 / config.price_divisor;
            assert!((actual_price - 1.10500).abs() < 0.00001);
        }

        #[test]
        fn test_usdjpy_price_conversion() {
            let config = resolve_instrument_config("USD", "JPY");
            let raw_price: u32 = 150250; // Represents 150.250
            let actual_price = raw_price as f64 / config.price_divisor;
            assert!((actual_price - 150.250).abs() < 0.001);
        }

        #[test]
        fn test_xauusd_price_conversion() {
            let config = resolve_instrument_config("XAU", "USD");
            let raw_price: u32 = 2050500; // Represents 2050.500
            let actual_price = raw_price as f64 / config.price_divisor;
            assert!((actual_price - 2050.500).abs() < 0.001);
        }

        #[test]
        fn test_usdrub_price_conversion() {
            let config = resolve_instrument_config("USD", "RUB");
            let raw_price: u32 = 90500; // Represents 90.500
            let actual_price = raw_price as f64 / config.price_divisor;
            assert!((actual_price - 90.500).abs() < 0.001);
        }
    }
}
