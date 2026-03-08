# Roadmap

## Strategy

1. Keep API contracts predictable and easy to upgrade.
2. Make CLI automation reliable in production jobs.
3. Improve first-class integrations for analytics workflows.
4. Measure performance/reliability continuously.

## Now (0.5.x stabilization)

- strengthen offline coverage for `fx_fetcher --config` combinations
- add more JSON schema-level assertions for machine output mode
- publish benchmark trend artifacts from scheduled CI
- expand docs cookbook for incremental and multi-asset workflows

## Next (0.6.0 ergonomics + integrations)

- typed CLI config loader with richer validation/recovery messages
- optional direct dataframe adapters behind feature flags
- improved export tooling (schema descriptors + compatibility checks)
- extension points for custom sink/output backends

## Later (0.7.0 scale + operations)

- smarter backfill planner for sparse instruments
- resumable multi-part jobs with stronger failure metadata
- better observability surfaces for orchestrators

## Evaluation tracks

- broader cross-library benchmark suite with reproducible fixtures
- optional multi-provider abstraction for fallback data sources
