# Benchmarks

This repository ships an offline benchmark harness focused on CPU-bound API paths.

## Goals

- Track regressions in parser and catalog hot paths.
- Keep API ergonomics improvements measurable.
- Provide a repeatable baseline before comparing against alternative libraries.

## What is benchmarked

`cargo bench --bench core_benchmarks` includes:

- `rate_request_parsing`: parsing pair/symbol requests in auto/strict modes
- `period_parsing`: typed period parsing and conversion
- `currency_pair_validation`: constructor validation under different batch sizes
- `catalog_parse_json`: universe JSON parsing cost
- `catalog_select_active_subset`: active-symbol selection cost
- `ticker_try_new_and_interval`: cheap construction/configuration path

## Run locally

```bash
cargo bench --bench core_benchmarks
```

Compile-only smoke check:

```bash
cargo bench --bench core_benchmarks --no-run
```

## Baseline table (fill from CI or local runs)

| Benchmark | Baseline (0.5.0) | Current | Delta |
|---|---:|---:|---:|
| rate_request_parsing/auto_mode | TBD | TBD | TBD |
| rate_request_parsing/pair_only | TBD | TBD | TBD |
| period_parsing/typed_period_from_str | TBD | TBD | TBD |
| currency_pair_validation/try_new_batch/16 | TBD | TBD | TBD |
| currency_pair_validation/try_new_batch/128 | TBD | TBD | TBD |
| currency_pair_validation/try_new_batch/1024 | TBD | TBD | TBD |
| catalog_parse_json | TBD | TBD | TBD |
| catalog_select_active_subset | TBD | TBD | TBD |
| ticker_try_new_and_interval | TBD | TBD | TBD |

## Cross-library comparison protocol

To compare with another library fairly:

1. Match data scope (same symbols, same date range, same sampling interval).
2. Separate network cost from local processing cost.
3. Warm caches before timing if the competitor does so by default.
4. Report median + p95 across at least 20 runs.
5. Capture memory profile (`/usr/bin/time -l` on macOS or `time -v` on Linux).

Suggested report columns:

| Library | Scenario | Median latency | p95 latency | RSS max | Notes |
|---|---|---:|---:|---:|---|
| dukascopy-fx | single pair @ timestamp | TBD | TBD | TBD | baseline |
| dukascopy-fx | 10 symbols / 30d backfill | TBD | TBD | TBD | baseline |
| competitor-X | single pair @ timestamp | TBD | TBD | TBD | matched setup |
| competitor-X | 10 symbols / 30d backfill | TBD | TBD | TBD | matched setup |

## CI recommendation

Run benchmark suite on a dedicated performance job (nightly/scheduled) and publish trend artifacts. Avoid mixing performance thresholds with per-PR unit tests, because hosted runner variance is high.
