# Memoria Next native matrix

Status date: 2026-08-13.

This document records the local Gate J evidence for the package and its N-API
binding. The current workstation is Windows, so the Windows row is executed
evidence. Linux and macOS rows remain open until the same matrix is run on
those hosts; a Windows pass is not a cross-platform pass.

## Toolchain and artifact

| Item             | Observed value                                                            |
| ---------------- | ------------------------------------------------------------------------- |
| Node             | `v24.19.0`                                                                |
| pnpm             | `11.20.0`                                                                 |
| Rust             | `rustc 1.97.1 (8bab26f4f 2026-07-14)`                                     |
| Rust host        | `x86_64-pc-windows-msvc`                                                  |
| Native target    | `win32-x64-msvc`                                                          |
| Native artifact  | [`native/index.win32-x64-msvc.node`](../native/index.win32-x64-msvc.node) |
| Artifact bytes   | `17,654,784`                                                              |
| Artifact SHA-256 | `C32DDA31457979700368887FB52CC9F967A9C537770348EAE35E578C872A94A1`        |
| Evidence commit  | `e02f7057b191d3675f1805d3c1d2e21c43198351`                                |

The package currently contains the Windows native artifact plus the platform
loader and declarations. No Linux or macOS binary is checked in by this
release-hardening pass.

## Matrix

The named Rust tests and TypeScript tests are the smallest focused evidence;
the full workspace and package suites were also run for the Windows row.

| Target                                               | Native build/load                   | Store lock                        | Create/revise/query                                                     | Restart recovery                                                                                        | Derived delete/rebuild                  | Provider-offline BaseReady                                             | Lease expiry                                                                 | Backup/restore                                               | Purge                                                                                    | Packed consumer                   | Result |
| ---------------------------------------------------- | ----------------------------------- | --------------------------------- | ----------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | --------------------------------------- | ---------------------------------------------------------------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------ | ---------------------------------------------------------------------------------------- | --------------------------------- | ------ |
| Windows `win32-x64-msvc`                             | PASS: `corepack pnpm verify:public` | PASS: `second_writer_is_rejected` | PASS: `runtime_can_commit_base_ready_memory_and_query_it`, public suite | PASS: `store_layout_opens_existing_store`, `first_schema_initialization_persists_version_across_reopen` | PASS: `deleting_derived_is_recoverable` | PASS: `base_ready_requires_zero_provider_work`, local-only egress test | PASS: `continuation_rejects_a_different_query_and_expiry`, cancellation test | PASS: `backup restore preserves the Store identity universe` | PASS: `completed_purge_removes_authority_visibility_and_managed_source_blob`, purge test | PASS: `corepack pnpm verify:pack` | PASS   |
| Linux `x86_64-unknown-linux-gnu`                     | NOT RUN: Linux host unavailable     | NOT RUN                           | NOT RUN                                                                 | NOT RUN                                                                                                 | NOT RUN                                 | NOT RUN                                                                | NOT RUN                                                                      | NOT RUN                                                      | NOT RUN                                                                                  | NOT RUN                           | OPEN   |
| macOS `aarch64-apple-darwin` / `x86_64-apple-darwin` | NOT RUN: macOS host unavailable     | NOT RUN                           | NOT RUN                                                                 | NOT RUN                                                                                                 | NOT RUN                                 | NOT RUN                                                                | NOT RUN                                                                      | NOT RUN                                                      | NOT RUN                                                                                  | NOT RUN                           | OPEN   |

## Windows evidence commands

The following commands completed successfully on the recorded Windows
toolchain:

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
corepack pnpm format:check
corepack pnpm lint
corepack pnpm typecheck
corepack pnpm test
corepack pnpm verify:docs
corepack pnpm verify:public
corepack pnpm verify:pack
```

The focused matrix names map to the following source tests:

- [Authority lifecycle tests](../crates/memoria-authority/tests/authority_lifecycle.rs)
  cover the exclusive writer lock, create/revise topology, and opening an
  existing Store.
- [Authority generation tests](../crates/memoria-authority/tests/generation_history.rs)
  cover schema persistence across reopen and crash-safe publication.
- [Derived rebuild tests](../crates/memoria-derived/tests/rebuild.rs) and
  [lease GC tests](../crates/memoria-derived/tests/leases_gc.rs) cover
  rebuildability and active lease protection.
- [BaseReady tests](../crates/memoria-derived/tests/base_ready.rs) cover the
  provider-free readiness condition.
- [Query session tests](../crates/memoria-query/tests/read_session.rs) cover
  continuation expiry and query binding.
- [Runtime smoke tests](../crates/memoria-runtime/tests/engine_smoke.rs)
  cover commit/query, provider work, projection-specific invalidation, and
  purge.
- [TypeScript security tests](../tests/security) cover provider egress,
  backup/restore, purge, import, resource limits, and scope protection.

The native build emits an MSVC linker notice about an absent `node.exe`
delay-load import. It is the expected N-API-RS Windows linker warning in this
workspace; the build and installed consumer both completed successfully.

## Gate J disposition

Windows is locally signed off for this matrix. Linux and macOS are not signed
off because their local runs require the corresponding operating systems and
native build hosts. GitHub Actions are intentionally disabled and are not used
as a substitute for those local runs.
