# Documentation Agent Instructions

## Scope and language

These rules apply to `docs/`. Human-facing documentation in this repository is
English to match the implementation, generated declarations, and architecture
plans. Keep this file and other AI-facing instructions concise technical
English.

## Evidence order

When prose conflicts with behavior, inspect executable checks, tests,
machine-readable manifests/schemas, implementation, and only then existing
documentation. Do not make an old page authoritative by changing code merely
to match it.

## Canonical ownership

- `src/index.ts`, `src/engine`, and generated declarations own public API facts.
- Rust crate code and tests own persistence, projection, query, and Adaptive
  facts.
- `package.json`, `Cargo.toml`, `rust-toolchain.toml`, and scripts own commands
  and tool versions.
- ADRs own measured decisions and explicit deferrals.

Topic documents should explain and link to these owners rather than creating a
second incompatible reference table.

## Drift rules

- Describe only the hard-reset Next surface. Do not reintroduce removed names,
  Store paths, or compatibility aliases as examples.
- Separate current behavior, measured evidence, and future decision gates.
- Keep internal links relative and verify them with `corepack pnpm verify:docs`.
- Run `corepack pnpm format:check` and `corepack pnpm verify:docs` after a docs
  change. The verifier also checks required topics and obsolete terms.
- Do not enable GitHub Actions as part of documentation verification.
