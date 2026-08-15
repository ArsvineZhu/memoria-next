# Testing and local gates

The repository uses local evidence gates. GitHub Actions are intentionally
disabled and must not be enabled or treated as a missing prerequisite.

## Toolchain

- Rust stable `1.97.1`, edition 2024, from `rust-toolchain.toml`;
- Node `>=24.18.1 <25`;
- pnpm `11.20.0` through Corepack;
- TypeScript `7.0.2`;
- N-API-RS CLI v3 conventions.

## Focused checks

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
corepack pnpm format:check
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
```

`test` compiles TypeScript tests into `dist-test` and runs every compiled
`.test.js` file with Node's built-in test runner. The tests cover the public
engine, cancellation/errors, providers, semantic work, feedback, governance,
scope protection, and Host/Agent workflows. `test:ts` remains the lower-level
TypeScript test compilation/runner script when a focused invocation is useful.

The D4–D9 focused regression suites are:

```text
cargo test -p memoria-runtime --test backup_consistency
cargo test -p memoria-runtime --test import_remap
cargo test -p memoria-runtime --test purge_recovery
cargo test -p memoria-types --test entity_ref
cargo test -p memoria-mdx --test referential_semantics
cargo test -p memoria-mdx --test profile_golden
cargo test -p memoria-authority --test concurrency
cargo test -p memoria-derived --test catalog_concurrency
cargo test -p memoria-adaptive --test concurrency
```

These suites cover restart identity, source-aware import remapping and
idempotency, purge recovery, single-source EntityRef grammar, raw HTML
rejection, merge/CAS integrity, and SQLite WAL/busy contention. The broader
workspace suite remains the authority for cross-crate integration.

## Package/release checks

```text
corepack pnpm build
corepack pnpm verify:docs
corepack pnpm verify:public
corepack pnpm verify:pack
```

`verify:public` rebuilds the native and TypeScript package and typechecks the
public entry point. `verify:pack` creates a real npm tarball in a temporary
directory, installs it into a separate consumer with scripts disabled, imports
the root and authoring subpath, opens a Store from outside the repository, and
executes create/query/close against the installed native loader.

## Deterministic measurements

The benchmark runners are diagnostics with checked-in fixtures, not claims of
production latency:

```text
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile lexical
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile fast
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile thorough
corepack pnpm exec tsx benchmarks/adaptive/replay.ts --adaptive=on
corepack pnpm exec tsx benchmarks/encryption/measure.ts
```

The incremental release harness is run with:

```text
corepack pnpm exec tsx benchmarks/incremental/run.ts
```

The retrieval benchmark is release evidence only through the real Runtime
path. Each run creates a temporary Store, writes through Authority, waits for
Derived/provider coverage, calls public `Memoria.query()`, and measures the
opt-in `QueryOperatorTrace`; it does not contain a TypeScript retrieval
simulator. Run the profile and acceptance gates with:

```text
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile fast
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile thorough
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced --ablation all
corepack pnpm exec tsx benchmarks/retrieval/run.ts --evaluate-acceptance
```

The runner writes ignored JSON/Markdown reports under
`benchmarks/retrieval/results/`. Acceptance compares the same labeled query
set and enforces zero hard-constraint violations, no more than a 0.005 nDCG
drop, at least +0.05 target Recall@10, no more than a 25% P95 increase, and no
unrequested provider calls. The current deterministic fixture keeps the
provider-free lexical profile as the Balanced default; advanced capabilities
remain explicit. See [runtime retrieval validation](reports/runtime-retrieval-validation.md).

The release validation runs the `fast`, `balanced`, and `thorough` retrieval
profiles plus the incremental harness. The profiles are diagnostic fixture
measurements, not production latency claims. The incremental harness must
prove no content embedding for formatting/comment-only, `validTo`, or
explicit-Tag-only edits, and prove that a Space move reuses the vector payload.

The release gate also checks that text-only queries perform zero provider calls,
semantic work is explicit and Rust-pending, raw MDX never reaches a provider,
hard constraints hold before ranking, current serving does not reparse
Authority sources, Adaptive state survives restart, backup/import/purge
recovery remains durable, and a packed consumer uses the repaired public API.

## Failure interpretation

Report the failing layer: formatting, lint/typecheck, Rust unit/integration,
native build, package installation, documentation drift, or an unavailable
external platform. A local Windows pass does not imply a Linux/macOS pass, and
an HTTP/build result does not prove installed-consumer or visual behavior.
