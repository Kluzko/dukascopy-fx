use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use dukascopy_fx::{
    CurrencyPair, InstrumentCatalog, Period, RateRequest, RequestParseMode, Ticker,
};
use std::str::FromStr;

fn bench_rate_request_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("rate_request_parsing");

    let inputs = [
        "EUR/USD",
        "eurusd",
        "XAUUSD",
        "AAPL",
        "USA500IDX",
        "BTCUSD",
        "SP500",
    ];

    group.bench_function("auto_mode", |b| {
        b.iter(|| {
            for input in &inputs {
                let parsed = RateRequest::parse_with_mode(black_box(input), RequestParseMode::Auto)
                    .expect("valid request should parse");
                black_box(parsed);
            }
        })
    });

    group.bench_function("pair_only", |b| {
        b.iter(|| {
            let pair_inputs = ["EUR/USD", "eurusd", "XAUUSD", "BTCUSD"];
            for input in &pair_inputs {
                let parsed =
                    RateRequest::parse_with_mode(black_box(input), RequestParseMode::PairOnly)
                        .expect("valid pair should parse");
                black_box(parsed);
            }
        })
    });

    group.finish();
}

fn bench_period_parsing(c: &mut Criterion) {
    let mut group = c.benchmark_group("period_parsing");
    let periods = ["1d", "7d", "1w", "1mo", "3mo", "1y"];

    group.bench_function("typed_period_from_str", |b| {
        b.iter(|| {
            for p in &periods {
                let parsed = Period::from_str(black_box(p)).expect("period should parse");
                black_box(parsed.to_duration().expect("period should convert"));
            }
        })
    });

    group.finish();
}

fn bench_currency_pair_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("currency_pair_validation");

    for size in [16usize, 128, 1024] {
        let pairs: Vec<(String, String)> = (0..size)
            .map(|idx| {
                let base = format!("S{:02}", idx % 90);
                let quote = format!("Q{:02}", idx % 90);
                (base, quote)
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::new("try_new_batch", size),
            &pairs,
            |b, pairs| {
                b.iter(|| {
                    for (base, quote) in pairs {
                        let pair = CurrencyPair::try_new(black_box(base), black_box(quote))
                            .expect("pair should validate");
                        black_box(pair);
                    }
                })
            },
        );
    }

    group.finish();
}

fn bench_catalog_operations(c: &mut Criterion) {
    let raw_catalog = include_str!("../config/universe.json");

    c.bench_function("catalog_parse_json", |b| {
        b.iter(|| {
            let catalog =
                InstrumentCatalog::from_json_str(black_box(raw_catalog)).expect("catalog parses");
            black_box(catalog.instruments.len());
        })
    });

    let catalog = InstrumentCatalog::from_json_str(raw_catalog).expect("catalog parses");
    let symbols = vec![
        "EURUSD".to_string(),
        "GBPUSD".to_string(),
        "XAUUSD".to_string(),
    ];

    c.bench_function("catalog_select_active_subset", |b| {
        b.iter(|| {
            let selected = catalog
                .select_active(black_box(&symbols))
                .expect("subset selection should work");
            black_box(selected.len());
        })
    });
}

fn bench_ticker_construction(c: &mut Criterion) {
    c.bench_function("ticker_try_new_and_interval", |b| {
        b.iter(|| {
            let ticker = Ticker::try_new("EUR", "USD")
                .expect("ticker should build")
                .interval(chrono::Duration::minutes(30));
            black_box(ticker.symbol());
        })
    });
}

criterion_group!(
    benches,
    bench_rate_request_parsing,
    bench_period_parsing,
    bench_currency_pair_validation,
    bench_catalog_operations,
    bench_ticker_construction
);
criterion_main!(benches);
