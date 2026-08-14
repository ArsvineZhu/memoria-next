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
corepack pnpm test:ts
```

`test:ts` compiles TypeScript tests into `dist-test` and runs every compiled
`.test.js` file with Node's built-in test runner. The tests cover the public
engine, cancellation/errors, providers, semantic work, feedback, governance,
scope protection, and Host/Agent workflows.

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
corepack pnpm exec tsx benchmarks/adaptive/replay.ts --adaptive=on
corepack pnpm exec tsx benchmarks/encryption/measure.ts
```

The incremental release harness is run with:

```text
corepack pnpm exec tsx benchmarks/incremental/run.ts
```

It must prove no content embedding for formatting/comment-only, `validTo`, or
explicit-Tag-only edits, and prove that a Space move reuses the vector payload.

## Failure interpretation

Report the failing layer: formatting, lint/typecheck, Rust unit/integration,
native build, package installation, documentation drift, or an unavailable
external platform. A local Windows pass does not imply a Linux/macOS pass, and
an HTTP/build result does not prove installed-consumer or visual behavior.
