# Roadmap

## Guiding priorities

1. Predictable API contracts and upgrade safety.
2. Production-friendly CLI behavior for batch jobs.
3. Better out-of-the-box analytics integrations.
4. Measurable performance and reliability regressions.

## 0.5.x (stabilization)

- tighten offline test coverage for CLI config combinations
- add more JSON schema-level assertions for machine output
- benchmark trend collection in scheduled CI
- docs pass: cookbook for incremental + multi-asset workflows

## 0.6.0 (ergonomics + integrations)

- typed CLI config loader with richer validation messages
- optional direct adapters for dataframe backends (feature-gated)
- improved export tooling (schema descriptors and compatibility checks)
- potential plugin hooks for custom sink/output backends

## 0.7.0 (scale + ops)

- smarter backfill planner for sparse instruments
- resumable multi-part jobs with stronger failure recovery metadata
- improved observability surfaces for orchestration systems

## Open evaluation tracks

- broader cross-library benchmark suite with reproducible fixture sets
- additional provider abstraction for multi-source fallback strategies
