use chrono::{TimeZone, Utc};
use dukascopy_fx::advanced::{DukascopyClientBuilder, PairResolutionMode};
use dukascopy_fx::{CurrencyPair, DukascopyError, Period, RateRequest, RequestParseMode};
use std::str::FromStr;

fn sample_ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 1, 3, 14, 45, 0).unwrap()
}

#[test]
fn test_rate_request_from_str_selects_pair_path() {
    let request: RateRequest = "EUR/USD".parse().unwrap();
    assert!(request.as_pair().is_some());
    assert_eq!(request.to_string(), "EUR/USD");
}

#[test]
fn test_rate_request_from_str_selects_symbol_path() {
    let request: RateRequest = "aapl".parse().unwrap();
    assert_eq!(request.as_symbol(), Some("AAPL"));
    assert_eq!(request.to_string(), "AAPL");
}

#[test]
fn test_rate_request_parse_with_mode_pair_only_rejects_symbol() {
    let err = RateRequest::parse_with_mode("AAPL", RequestParseMode::PairOnly).unwrap_err();
    assert!(matches!(err, DukascopyError::InvalidRequest(_)));
}

#[test]
fn test_period_type_is_public_and_parsable() {
    assert_eq!(Period::from_str("1d").unwrap(), Period::Days(1));
}

#[test]
fn test_rate_request_from_currency_pair() {
    let pair = CurrencyPair::new("usd", "pln");
    let request: RateRequest = pair.into();

    let pair = request.as_pair().unwrap();
    assert_eq!(pair.from(), "USD");
    assert_eq!(pair.to(), "PLN");
}

#[tokio::test]
async fn test_get_rate_for_input_rejects_empty_request() {
    let err = dukascopy_fx::get_rate_for_input(" ", sample_ts())
        .await
        .unwrap_err();
    assert!(matches!(err, DukascopyError::InvalidRequest(_)));
}

#[tokio::test]
async fn test_get_rate_for_input_rejects_invalid_symbol() {
    let err = dukascopy_fx::get_rate_for_input("BAD$", sample_ts())
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        DukascopyError::InvalidCurrencyCode { code, .. } if code == "BAD$"
    ));
}

#[tokio::test]
async fn test_get_rate_for_input_rejects_invalid_pair() {
    let err = dukascopy_fx::get_rate_for_input("EUR/US$", sample_ts())
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        DukascopyError::InvalidCurrencyCode { code, .. } if code == "US$"
    ));
}

#[tokio::test]
async fn test_get_rate_for_request_rejects_invalid_pair_before_network() {
    let request = RateRequest::pair("BAD$", "USD");
    let err = dukascopy_fx::get_rate_for_request(&request, sample_ts())
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        DukascopyError::InvalidCurrencyCode { code, .. } if code == "BAD$"
    ));
}

#[tokio::test]
async fn test_client_symbol_request_requires_default_quote() {
    let client = DukascopyClientBuilder::new().build();
    let err = client
        .get_exchange_rate_for_symbol("AAPL", sample_ts())
        .await
        .unwrap_err();

    assert!(matches!(err, DukascopyError::MissingDefaultQuoteCurrency));
}

#[tokio::test]
async fn test_client_symbol_request_respects_explicit_only_mode() {
    let client = DukascopyClientBuilder::new()
        .default_quote_currency("USD")
        .pair_resolution_mode(PairResolutionMode::ExplicitOnly)
        .build();

    let err = client
        .get_exchange_rate_for_symbol("AAPL", sample_ts())
        .await
        .unwrap_err();

    assert!(matches!(err, DukascopyError::PairResolutionDisabled));
}

#[tokio::test]
async fn test_client_symbol_request_rejects_invalid_symbol_before_network() {
    let client = DukascopyClientBuilder::new()
        .default_quote_currency("USD")
        .build();

    let err = client
        .get_exchange_rate_for_symbol("BAD$", sample_ts())
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        DukascopyError::InvalidCurrencyCode { code, .. } if code == "BAD$"
    ));
}

#[test]
fn test_builder_normalizes_aliases_and_bridges() {
    let client = DukascopyClientBuilder::new()
        .bridge_currencies(&["usd", "EUR", "usd", "  "])
        .code_alias("aapl", "aaplus")
        .code_alias(" ", "BAD")
        .build();

    assert_eq!(
        client.config().bridge_currencies,
        vec!["USD".to_string(), "EUR".to_string()]
    );
    assert_eq!(
        client.config().code_aliases.get("AAPL"),
        Some(&"AAPLUS".to_string())
    );
    assert!(!client.config().code_aliases.contains_key(""));
}
