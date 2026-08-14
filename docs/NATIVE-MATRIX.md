# Memoria Next native matrix

Status date: 2026-08-14.

This document records the local Gate J evidence for the package and its N-API
binding. The current workstation is Windows, so the Windows row is executed
evidence. Linux and macOS rows remain open until the same matrix is run on
those hosts; a Windows pass is not a cross-platform pass.

## Toolchain and artifact

| Item                     | Observed value                                                            |
| ------------------------ | ------------------------------------------------------------------------- |
| Node                     | `v24.19.0`                                                                |
| pnpm                     | `11.20.0`                                                                 |
| Rust                     | `rustc 1.97.1 (8bab26f4f 2026-07-14)`                                     |
| Rust host                | `x86_64-pc-windows-msvc`                                                  |
| Native target            | `win32-x64-msvc`                                                          |
| Native artifact          | [`native/index.win32-x64-msvc.node`](../native/index.win32-x64-msvc.node) |
| Artifact bytes           | `20,240,896`                                                              |
| Artifact SHA-256         | `F971B743B53D74D283E1C0FA2DD4BE488723727B33F199E040E4FA9907346C4B`        |
| Evidence source baseline | `622d89245c5e49eaeaaf3a8430a9ada7f0a338c5`                                |

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

The remediation-specific Windows checks are also green:

- `backup_consistency`, `import_remap`, and `purge_recovery` cover online
  backup/restore, source-aware portable import, idempotent remapping, and
  resumable purge cleanup;
- `entity_ref` and `referential_semantics` cover the canonical EntityRef
  grammar, per-element Core kinds, and raw HTML/runtime rejection;
- the Authority, Derived, and Adaptive concurrency suites cover SQLite WAL,
  busy-timeout retries, and restart-safe durable state;
- CAS corruption, merge-parent canonicalization, provider policy, semantic
  publication, and packed-consumer checks are included in the workspace and
  TypeScript suites.

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
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile fast
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile thorough
corepack pnpm exec tsx benchmarks/incremental/run.ts
corepack pnpm exec tsx benchmarks/adaptive/replay.ts --adaptive=on
```

The final fixture measurements were: fast `recall@k=1`, `MRR=1`,
`NDCG@k=0.9532`, zero provider calls and zero hard-constraint violations;
balanced had the same quality and safety values; thorough had `recall@k=1`,
`MRR=0.9375`, `NDCG@k=0.9387`, eight ANN/provider/rerank calls, and zero
hard-constraint violations. The incremental harness reported two embedding,
16 enrichment, and zero rerank calls; formatting/comment-only, EntityRef-only,
Tag-only, temporal, relation, and source-locator changes did not rebuild
content embeddings, while a Space move reused the immutable vector payload.

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
