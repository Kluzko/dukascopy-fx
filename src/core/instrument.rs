//! Instrument configuration for price scaling and decimal precision.

use std::borrow::Cow;
use std::collections::HashMap;

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
    #[inline]
    pub const fn new(price_divisor: f64, decimal_places: u32) -> Self {
        Self {
            price_divisor,
            decimal_places,
        }
    }

    /// Standard forex pair configuration (5 decimal places)
    pub const STANDARD: Self = Self::new(DIVISOR_5_DECIMALS, 5);

    /// JPY forex pair configuration (3 decimal places)
    pub const JPY: Self = Self::new(DIVISOR_3_DECIMALS, 3);

    /// Metals configuration (3 decimal places)
    pub const METALS: Self = Self::new(DIVISOR_3_DECIMALS, 3);

    /// RUB pairs configuration (3 decimal places)
    pub const RUB: Self = Self::new(DIVISOR_3_DECIMALS, 3);

    /// Index configuration (2 decimal places)
    pub const INDEX: Self = Self::new(DIVISOR_2_DECIMALS, 2);
}

impl Default for InstrumentConfig {
    fn default() -> Self {
        Self::STANDARD
    }
}

/// Categories of currencies/instruments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurrencyCategory {
    Standard,
    Jpy,
    Rub,
    Metal,
    Unknown,
}

impl CurrencyCategory {
    /// Categorizes a currency code
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
            "USD" | "EUR" | "GBP" | "AUD" | "NZD" | "CAD" | "CHF" | "SEK" | "NOK" | "DKK"
            | "SGD" | "HKD" | "MXN" | "ZAR" | "TRY" | "PLN" | "CZK" | "HUF" | "CNH" | "CNY"
            | "INR" | "THB" | "KRW" | "TWD" | "BRL" | "ILS" => Self::Standard,
            _ => Self::Unknown,
        }
    }

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
pub fn resolve_instrument_config(from: &str, to: &str) -> InstrumentConfig {
    let from_cat = CurrencyCategory::from_code(from);
    let to_cat = CurrencyCategory::from_code(to);

    match (from_cat, to_cat) {
        (CurrencyCategory::Metal, _) | (_, CurrencyCategory::Metal) => InstrumentConfig::METALS,
        (CurrencyCategory::Jpy, _) | (_, CurrencyCategory::Jpy) => InstrumentConfig::JPY,
        (CurrencyCategory::Rub, _) | (_, CurrencyCategory::Rub) => InstrumentConfig::RUB,
        _ => InstrumentConfig::STANDARD,
    }
}

/// Trait for types that can provide instrument configuration
pub trait HasInstrumentConfig {
    fn instrument_config(&self) -> InstrumentConfig;

    #[inline]
    fn price_divisor(&self) -> f64 {
        self.instrument_config().price_divisor
    }

    #[inline]
    fn decimal_places(&self) -> u32 {
        self.instrument_config().decimal_places
    }
}

/// Trait for providing instrument configurations.
pub trait InstrumentProvider: Send + Sync {
    fn get_config(&self, from: &str, to: &str) -> InstrumentConfig;
}

/// Default instrument provider using automatic detection.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultInstrumentProvider;

impl InstrumentProvider for DefaultInstrumentProvider {
    fn get_config(&self, from: &str, to: &str) -> InstrumentConfig {
        resolve_instrument_config(from, to)
    }
}

/// Instrument provider with custom overrides.
#[derive(Debug, Clone, Default)]
pub struct OverrideInstrumentProvider {
    overrides: HashMap<String, InstrumentConfig>,
}

impl OverrideInstrumentProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_override(&mut self, from: &str, to: &str, config: InstrumentConfig) -> &mut Self {
        let key = format!("{}{}", from.to_ascii_uppercase(), to.to_ascii_uppercase());
        self.overrides.insert(key, config);
        self
    }

    pub fn remove_override(&mut self, from: &str, to: &str) -> &mut Self {
        let key = format!("{}{}", from.to_ascii_uppercase(), to.to_ascii_uppercase());
        self.overrides.remove(&key);
        self
    }

    pub fn has_override(&self, from: &str, to: &str) -> bool {
        let key = format!("{}{}", from.to_ascii_uppercase(), to.to_ascii_uppercase());
        self.overrides.contains_key(&key)
    }

    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }
}

impl InstrumentProvider for OverrideInstrumentProvider {
    fn get_config(&self, from: &str, to: &str) -> InstrumentConfig {
        let key = format!("{}{}", from.to_ascii_uppercase(), to.to_ascii_uppercase());
        self.overrides
            .get(&key)
            .copied()
            .unwrap_or_else(|| resolve_instrument_config(from, to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_currency_category() {
        assert_eq!(CurrencyCategory::from_code("JPY"), CurrencyCategory::Jpy);
        assert_eq!(CurrencyCategory::from_code("jpy"), CurrencyCategory::Jpy);
        assert_eq!(CurrencyCategory::from_code("XAU"), CurrencyCategory::Metal);
        assert_eq!(
            CurrencyCategory::from_code("USD"),
            CurrencyCategory::Standard
        );
        assert_eq!(
            CurrencyCategory::from_code("XYZ"),
            CurrencyCategory::Unknown
        );
    }

    #[test]
    fn test_resolve_config() {
        assert_eq!(
            resolve_instrument_config("EUR", "USD"),
            InstrumentConfig::STANDARD
        );
        assert_eq!(
            resolve_instrument_config("USD", "JPY"),
            InstrumentConfig::JPY
        );
        assert_eq!(
            resolve_instrument_config("XAU", "USD"),
            InstrumentConfig::METALS
        );
        assert_eq!(
            resolve_instrument_config("USD", "RUB"),
            InstrumentConfig::RUB
        );
    }

    #[test]
    fn test_override_provider() {
        let mut provider = OverrideInstrumentProvider::new();
        provider.add_override("BTC", "USD", InstrumentConfig::new(100.0, 2));

        assert_eq!(provider.get_config("BTC", "USD").price_divisor, 100.0);
        assert_eq!(
            provider.get_config("EUR", "USD"),
            InstrumentConfig::STANDARD
        );
    }

    #[test]
    fn test_config_constants() {
        assert_eq!(InstrumentConfig::STANDARD.price_divisor, 100_000.0);
        assert_eq!(InstrumentConfig::JPY.price_divisor, 1_000.0);
        assert_eq!(InstrumentConfig::METALS.price_divisor, 1_000.0);
    }
}
