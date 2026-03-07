# Release Notes

## 0.5.0 (2026-03-07)

This release focuses on API safety, CLI UX hardening, and adoption tooling.

### Highlights

- Stronger request parsing model (`RequestParseMode`) and typed periods (`Period`).
- Improved constructor safety (`try_new`-first path in docs and APIs).
- Structured transport errors and cleaner error semantics.
- Configurable client policies (download/backtrack/concurrency).
- Lighter default dependency surface (optional parquet stack).

### CLI (`fx_fetcher`) improvements

- strict option parsing (unknown flags and missing values are explicit errors)
- explicit output mode required (`--out` or `--no-output`)
- safe checkpoint behavior in no-output mode
- `--config PATH.toml` support for command defaults
- `--json` machine-readable output mode
- `export` supports `--has-headers`
- discovery scraping now uses parser-based XML/HTML extraction

### Quality and stability

- public API snapshot test (`tests/public_api_snapshot_test.rs`)
- API stability policy (`docs/API_STABILITY.md`)
- CI matrix: stable, beta, MSRV + lint/tests/docs + supply-chain checks
- benchmark harness with repeatable methodology (`docs/BENCHMARKS.md`)

### Data-science integrations

- new interop adapter: `FlatExchangeRow`, `flatten_row`, `flatten_rows`
- ready guides/snippets for Pandas and Polars (`docs/INTEGRATIONS.md`)

### Upgrade notes

- crate version changed from `0.4.1` to `0.5.0`
- release branch renamed from `release/0.4.1` to `release/0.5.0`
- CLI scripts should prefer `--config`/`--json` for automation reliability
