use chrono::{DateTime, Duration, Utc};
use dukascopy_fx::advanced::{
    ClientConfig, ConfiguredClient, DukascopyClientBuilder, PairResolutionMode,
};
use dukascopy_fx::{
    CurrencyExchange, CurrencyPair, DukascopyError, Period, RateRequest, RequestParseMode, Ticker,
};
use std::future::Future;

fn assert_future_exchange<F>(future: F)
where
    F: Future<Output = dukascopy_fx::Result<CurrencyExchange>>,
{
    std::mem::drop(future);
}

fn assert_future_series<F>(future: F)
where
    F: Future<Output = dukascopy_fx::Result<Vec<CurrencyExchange>>>,
{
    std::mem::drop(future);
}

#[test]
fn public_api_snapshot_types_and_constants() {
    let _: Period = Period::Days(1);
    let _: RequestParseMode = RequestParseMode::Auto;
    let _: PairResolutionMode = PairResolutionMode::ExplicitOnly;
    let _: usize = dukascopy_fx::DEFAULT_DOWNLOAD_CONCURRENCY;

    let cfg: ClientConfig = ClientConfig::default();
    assert!(cfg.max_in_flight_requests >= 1);

    let _: fn() -> DukascopyClientBuilder = DukascopyClientBuilder::new;
    let _: fn(DukascopyClientBuilder) -> ConfiguredClient = DukascopyClientBuilder::build;
}

#[test]
fn public_api_snapshot_sync_signatures() {
    let _: fn(&str, &str) -> Result<Ticker, DukascopyError> = Ticker::try_new;
    let _: fn(&str, &str) -> Ticker = Ticker::new;
    let _: fn(&str) -> Result<Ticker, DukascopyError> = Ticker::parse;
    let _: fn(Ticker, Duration) -> Ticker = Ticker::interval;

    let pair_result: Result<CurrencyPair, DukascopyError> = CurrencyPair::try_new("EUR", "USD");
    assert!(pair_result.is_ok());
    let pair: CurrencyPair = CurrencyPair::new("EUR", "USD");
    assert_eq!(pair.as_symbol(), "EURUSD");

    let req_pair: RateRequest = RateRequest::pair("EUR", "USD");
    assert!(req_pair.as_pair().is_some());
    let req_symbol = RateRequest::symbol("AAPL");
    assert!(req_symbol.is_ok());
    let req_mode = RateRequest::parse_with_mode("EUR/USD", RequestParseMode::PairOnly);
    assert!(req_mode.is_ok());
}

#[test]
fn public_api_snapshot_async_signatures() {
    let ts = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();

    assert_future_exchange(dukascopy_fx::get_rate("EUR", "USD", ts));

    let pair = CurrencyPair::new("EUR", "USD");
    assert_future_exchange(dukascopy_fx::get_rate_for_pair(&pair, ts));

    let request = RateRequest::pair("EUR", "USD");
    assert_future_exchange(dukascopy_fx::get_rate_for_request(&request, ts));

    assert_future_exchange(dukascopy_fx::get_rate_for_input("EUR/USD", ts));
    assert_future_exchange(dukascopy_fx::get_rate_for_input_with_mode(
        "EUR/USD",
        RequestParseMode::PairOnly,
        ts,
    ));
    assert_future_exchange(dukascopy_fx::get_rate_for_symbol("AAPL", ts));
    assert_future_exchange(dukascopy_fx::get_rate_in_quote("AAPL", "USD", ts));
    assert_future_series(dukascopy_fx::get_rates_range(
        "EUR",
        "USD",
        ts - Duration::hours(4),
        ts,
        Duration::hours(1),
    ));
}
