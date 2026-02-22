//! Instrument catalog and universe loading utilities.

use crate::error::DukascopyError;
use crate::models::CurrencyPair;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Asset class for an instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetClass {
    Fx,
    Metal,
    Equity,
    Index,
    Commodity,
    Crypto,
    Other,
}

/// Instrument metadata used by the fetcher.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstrumentDefinition {
    /// Full symbol in Dukascopy format, e.g. `EURUSD`.
    pub symbol: String,
    /// Base instrument code.
    pub base: String,
    /// Quote instrument code.
    pub quote: String,
    /// Instrument class.
    pub asset_class: AssetClass,
    /// Divisor used to decode raw prices.
    pub price_divisor: f64,
    /// Number of decimal places for formatting.
    pub decimal_places: u32,
    /// Whether instrument is active in the universe.
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

impl InstrumentDefinition {
    /// Returns pair representation for this instrument.
    pub fn pair(&self) -> CurrencyPair {
        CurrencyPair::new(&self.base, &self.quote)
    }
}

/// Collection of instruments used by the fetcher.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InstrumentCatalog {
    /// Catalog entries.
    pub instruments: Vec<InstrumentDefinition>,
    /// Optional code aliases, e.g. `AAPL -> AAPLUS`.
    #[serde(default)]
    pub code_aliases: HashMap<String, String>,
}

impl InstrumentCatalog {
    /// Load catalog from a JSON file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, DukascopyError> {
        let content = fs::read_to_string(path.as_ref()).map_err(|err| {
            DukascopyError::Unknown(format!(
                "Failed to read instrument universe file '{}': {}",
                path.as_ref().display(),
                err
            ))
        })?;

        Self::from_json_str(&content)
    }

    /// Parse catalog from JSON content.
    pub fn from_json_str(content: &str) -> Result<Self, DukascopyError> {
        let catalog: Self = serde_json::from_str(content).map_err(|err| {
            DukascopyError::InvalidRequest(format!("Invalid instrument universe JSON: {}", err))
        })?;
        catalog.validate()?;
        Ok(catalog)
    }

    /// Returns all active instruments.
    pub fn active_instruments(&self) -> Vec<&InstrumentDefinition> {
        self.instruments.iter().filter(|i| i.active).collect()
    }

    /// Finds instrument by symbol (case-insensitive).
    pub fn find(&self, symbol: &str) -> Option<&InstrumentDefinition> {
        let symbol = symbol.trim().to_ascii_uppercase();
        self.instruments.iter().find(|i| i.symbol == symbol)
    }

    /// Returns active instruments matching provided symbols.
    pub fn select_active(
        &self,
        symbols: &[String],
    ) -> Result<Vec<&InstrumentDefinition>, DukascopyError> {
        if symbols.is_empty() {
            return Ok(self.active_instruments());
        }

        let mut selected = Vec::with_capacity(symbols.len());
        for symbol in symbols {
            let instrument = self.find(symbol).ok_or_else(|| {
                DukascopyError::InvalidRequest(format!(
                    "Instrument '{}' not found in catalog",
                    symbol
                ))
            })?;
            if !instrument.active {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Instrument '{}' is marked as inactive",
                    symbol
                )));
            }
            selected.push(instrument);
        }
        Ok(selected)
    }

    /// Resolves code alias to canonical code.
    pub fn resolve_code_alias(&self, code: &str) -> String {
        let aliases = self.normalized_code_aliases();
        resolve_alias_chain(&aliases, code.trim().to_ascii_uppercase())
    }

    /// Returns normalized alias map.
    pub fn normalized_code_aliases(&self) -> HashMap<String, String> {
        let aliases: HashMap<String, String> = self
            .code_aliases
            .iter()
            .map(|(alias, canonical)| {
                (
                    alias.trim().to_ascii_uppercase(),
                    canonical.trim().to_ascii_uppercase(),
                )
            })
            .collect();

        aliases
            .keys()
            .map(|alias| (alias.clone(), resolve_alias_chain(&aliases, alias.clone())))
            .collect()
    }

    fn validate(&self) -> Result<(), DukascopyError> {
        if self.instruments.is_empty() {
            return Err(DukascopyError::InvalidRequest(
                "Instrument catalog cannot be empty".to_string(),
            ));
        }

        for instrument in &self.instruments {
            if instrument.symbol.len() < 6 {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Invalid symbol '{}' in catalog",
                    instrument.symbol
                )));
            }

            if !is_valid_instrument_code(&instrument.base)
                || !is_valid_instrument_code(&instrument.quote)
            {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Invalid base/quote for symbol '{}'",
                    instrument.symbol
                )));
            }

            let expected_symbol = format!(
                "{}{}",
                instrument.base.to_ascii_uppercase(),
                instrument.quote.to_ascii_uppercase()
            );
            if instrument.symbol.to_ascii_uppercase() != expected_symbol {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Invalid symbol '{}' in catalog: expected '{}'",
                    instrument.symbol, expected_symbol
                )));
            }

            if instrument.price_divisor <= 0.0 {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Invalid price_divisor for symbol '{}'",
                    instrument.symbol
                )));
            }
        }

        let known_codes: HashSet<String> = self
            .instruments
            .iter()
            .flat_map(|instrument| {
                [
                    instrument.base.trim().to_ascii_uppercase(),
                    instrument.quote.trim().to_ascii_uppercase(),
                ]
            })
            .collect();

        for (alias, canonical) in self.normalized_code_aliases() {
            if !is_valid_instrument_code(&alias) || !is_valid_instrument_code(&canonical) {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Invalid code alias mapping '{} -> {}'",
                    alias, canonical
                )));
            }
            if !known_codes.contains(&canonical) {
                return Err(DukascopyError::InvalidRequest(format!(
                    "Alias canonical '{}' is not present in instrument catalog",
                    canonical
                )));
            }
        }

        Ok(())
    }
}

fn is_valid_instrument_code(code: &str) -> bool {
    let len = code.len();
    (2..=12).contains(&len) && code.chars().all(|ch| ch.is_ascii_alphanumeric())
}

fn resolve_alias_chain(aliases: &HashMap<String, String>, initial: String) -> String {
    let mut current = initial;
    let mut visited = HashSet::new();

    while let Some(next) = aliases.get(&current) {
        if !visited.insert(current.clone()) {
            break;
        }

        if next == &current {
            break;
        }

        current = next.clone();
    }

    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_catalog() {
        let json = r#"
        {
          "instruments": [
            {
              "symbol": "EURUSD",
              "base": "EUR",
              "quote": "USD",
              "asset_class": "fx",
              "price_divisor": 100000.0,
              "decimal_places": 5,
              "active": true
            }
          ]
        }
        "#;

        let catalog = InstrumentCatalog::from_json_str(json).unwrap();
        assert_eq!(catalog.instruments.len(), 1);
        assert_eq!(catalog.active_instruments().len(), 1);
    }

    #[test]
    fn test_find_case_insensitive() {
        let json = r#"
        {
          "instruments": [
            {
              "symbol": "USDJPY",
              "base": "USD",
              "quote": "JPY",
              "asset_class": "fx",
              "price_divisor": 1000.0,
              "decimal_places": 3,
              "active": true
            }
          ]
        }
        "#;

        let catalog = InstrumentCatalog::from_json_str(json).unwrap();
        assert!(catalog.find("usdjpy").is_some());
    }

    #[test]
    fn test_catalog_allows_non_three_char_instrument_codes() {
        let json = r#"
        {
          "instruments": [
            {
              "symbol": "DE40USD",
              "base": "DE40",
              "quote": "USD",
              "asset_class": "index",
              "price_divisor": 100.0,
              "decimal_places": 2,
              "active": true
            }
          ]
        }
        "#;

        let catalog = InstrumentCatalog::from_json_str(json).unwrap();
        assert_eq!(catalog.instruments.len(), 1);
        assert_eq!(catalog.instruments[0].symbol, "DE40USD");
    }

    #[test]
    fn test_catalog_code_aliases() {
        let json = r#"
        {
          "instruments": [
            {
              "symbol": "AAPLUSUSD",
              "base": "AAPLUS",
              "quote": "USD",
              "asset_class": "equity",
              "price_divisor": 1000.0,
              "decimal_places": 2,
              "active": true
            }
          ],
          "code_aliases": {
            "AAPL": "AAPLUS"
          }
        }
        "#;

        let catalog = InstrumentCatalog::from_json_str(json).unwrap();
        assert_eq!(catalog.resolve_code_alias("aapl"), "AAPLUS");
        assert_eq!(catalog.resolve_code_alias("msft"), "MSFT");
    }

    #[test]
    fn test_catalog_alias_chain_resolution() {
        let json = r#"
        {
          "instruments": [
            {
              "symbol": "USA500IDXUSD",
              "base": "USA500IDX",
              "quote": "USD",
              "asset_class": "index",
              "price_divisor": 1000.0,
              "decimal_places": 2,
              "active": true
            }
          ],
          "code_aliases": {
            "SP500": "US500",
            "US500": "USA500IDX"
          }
        }
        "#;

        let catalog = InstrumentCatalog::from_json_str(json).unwrap();
        assert_eq!(catalog.resolve_code_alias("SP500"), "USA500IDX");
    }

    #[test]
    fn test_catalog_alias_canonical_must_exist_in_catalog_codes() {
        let json = r#"
        {
          "instruments": [
            {
              "symbol": "EURUSD",
              "base": "EUR",
              "quote": "USD",
              "asset_class": "fx",
              "price_divisor": 100000.0,
              "decimal_places": 5,
              "active": true
            }
          ],
          "code_aliases": {
            "SPOT": "MISSING"
          }
        }
        "#;

        let error = InstrumentCatalog::from_json_str(json).unwrap_err();
        assert!(error
            .to_string()
            .contains("not present in instrument catalog"));
    }
}
