# API Stability Policy

This project follows [Semantic Versioning](https://semver.org/).

## Compatibility contract

For releases `>= 0.5.0`:

- Patch (`0.5.x`): bugfixes and internal improvements only; no breaking public API changes.
- Minor (`0.x+1.0`): may add APIs and behavior, but should remain source-compatible whenever possible.
- Major (`1.0.0+`): required for intentional breaking changes.

## What is considered public API

- Re-exports from `src/lib.rs`.
- Public modules/types/functions available without `pub(crate)`.
- Public behavior documented in README and rustdoc examples.
- CLI interface of `fx_fetcher` (commands, flags, exit semantics).

## Deprecation policy

When replacing a public API:

1. Mark old API with `#[deprecated(note = "...")]`.
2. Document migration path in rustdoc and changelog.
3. Keep deprecated API for at least one minor release before removal.
4. Remove only in a planned release with explicit release note entry.

## Change classification examples

Breaking (requires major):

- removing/renaming a public function/type/variant
- changing argument types or return types of public functions
- changing default CLI behavior in incompatible way

Non-breaking:

- adding new public functions/types/flags
- adding optional feature flags
- adding new error variants (if enum is already treated as non-exhaustive in consumer matching policy)

## Enforcement in this repository

- `tests/public_api_snapshot_test.rs` acts as a compile-time snapshot of key signatures.
- `tests/public_api_offline_test.rs` validates behavior-level contract for common flows.
- Changelog must include migration notes for deprecated APIs.

## Contributor checklist

Before merging public API changes:

1. Run `cargo test --test public_api_snapshot_test`.
2. Run `cargo test --test public_api_offline_test`.
3. Update README/rustdoc/changelog.
4. If breaking, schedule for next major and add migration section.
