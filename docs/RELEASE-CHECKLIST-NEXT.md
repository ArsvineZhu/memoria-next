# Memoria Next release checklist

This is the local release-hardening checklist for the hard-reset Next package.
It records completed evidence without implying that an unexecuted operating
system has passed. GitHub Actions are intentionally disabled for this line.

## Package and toolchain

- [x] Package name is `@arsvinezhu/memoria` and the public exports are the root
      entry point and `./authoring`.
- [x] Node `v24.19.0` satisfies the declared `>=24.18.1 <25` range.
- [x] pnpm `11.20.0` and Rust `1.97.1` match the repository baseline.
- [x] The Windows native artifact is built for `win32-x64-msvc` and is listed
      in [NATIVE-MATRIX.md](NATIVE-MATRIX.md).
- [x] A real npm tarball consumer installs outside the repository, imports both
      public entry points, creates a Store, queries it, and closes it.

## Local verification gates

The following checks passed on 2026-08-13 from commit
`e02f7057b191d3675f1805d3c1d2e21c43198351`:

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [x] `cargo test --workspace`
- [x] `corepack pnpm format:check`
- [x] `corepack pnpm lint`
- [x] `corepack pnpm typecheck`
- [x] `corepack pnpm test` — 44 TypeScript tests passed after the Rust suite.
- [x] `corepack pnpm verify:docs`
- [x] `corepack pnpm verify:public`
- [x] `corepack pnpm verify:pack`
- [x] `corepack pnpm exec tsx benchmarks/incremental/run.ts`

The incremental harness recorded zero content-embedding calls for formatting,
entity-reference-only, explicit-Tag-only, temporal-boundary, relation, and
source-locator changes. Visible prose and entity-surface changes each caused
one content-embedding call, and the Space-move case reused the immutable vector
payload. The measured values are in [ADR 0005](decisions/0005-release-performance-baseline.md).

## Gate J platform status

- [x] Windows local matrix: signed off. See [NATIVE-MATRIX.md](NATIVE-MATRIX.md)
      for the target, artifact hash, focused tests, and full commands.
- [ ] Linux local matrix: not executed on the Windows workstation.
- [ ] macOS local matrix: not executed on the Windows workstation.
- [ ] Cross-platform release sign-off: remains open until both local matrices
      are executed on their native hosts.

The open Linux/macOS items are environment-bound evidence gaps, not claims of
runtime failure. They must be closed by running the same matrix on those hosts;
an external CI result would not replace the intended local release gate.

## Operational boundaries

- [x] Provider egress remains explicit and configurable; provider-offline
      BaseReady behavior is covered by focused Rust and TypeScript tests.
- [x] Read sessions and continuation expiry are bounded and covered.
- [x] Backup/restore preserves the Store identity universe and purge remains
      resumable and recoverable.
- [x] Store-wide encryption at rest is not advertised by this release line;
      the limitation is recorded in [ADR 0004](decisions/0004-at-rest-encryption.md).
- [x] GitHub Actions remain disabled by design.
