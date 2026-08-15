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
- [x] The recorded Windows artifact is 20,240,896 bytes with SHA-256
      `F971B743B53D74D283E1C0FA2DD4BE488723727B33F199E040E4FA9907346C4B`.
- [x] A real npm tarball consumer installs outside the repository, imports both
      public entry points, creates a Store, queries it, and closes it.

## Local verification gates

The following checks passed on 2026-08-14 from the runtime evidence commit
`9f4240873d562ea781358396f91056ed2fb72901`:

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [x] `cargo test --workspace -- --test-threads=1`
- [x] `corepack pnpm format:check`
- [x] `corepack pnpm lint`
- [x] `corepack pnpm typecheck`
- [x] `corepack pnpm test:ts` — 100 TypeScript tests passed after the Rust suite.
- [x] `corepack pnpm verify:docs`
- [x] `corepack pnpm verify:public`
- [x] `corepack pnpm verify:pack`
- [x] `corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile fast`
- [x] `corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced`
- [x] `corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile thorough`
- [x] `corepack pnpm exec tsx benchmarks/incremental/run.ts`
- [x] `corepack pnpm exec tsx benchmarks/adaptive/replay.ts --adaptive=on`

The runtime retrieval fixture and acceptance results are recorded in
[runtime retrieval validation](reports/runtime-retrieval-validation.md).
The benchmark uses the public Runtime path and real `QueryOperatorTrace`; its
deterministic local-provider metrics are evidence of execution and regression
behavior, not production-scale latency claims. The acceptance runner kept the
provider-free lexical profile as the Balanced default and retained advanced
operators as explicit capabilities.

The default parallel `cargo test --workspace` wrapper was also attempted, but
this Windows workstation intermittently returned OS error 33 while temporary
Derived/Tantivy files were being created or removed. The full Rust workspace
passed with one test thread; this is an environment-specific test execution
note, not a serving assertion failure.

The incremental harness recorded two embedding, 16 enrichment, and zero
rerank calls. Formatting/comment-only, EntityRef-only, explicit-Tag-only,
temporal-boundary, relation, and source-locator changes caused no content
embedding rebuild; visible prose and entity-surface changes caused one each,
and the Space-move case reused the immutable vector payload. Adaptive replay
reported zero cross-Space contamination, zero exact-lookup exclusion, zero
unused Top-K events, and zero base-relevance violations. The measured values
are also summarized in [ADR 0013](decisions/0013-runtime-remediation-complete.md).

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
- [x] Backup/restore preserves the Store identity universe; portable import
      rewrites internal references with durable idempotency; and purge remains
      resumable and recoverable.
- [x] CAS corruption is surfaced as `CORRUPTION`; raw HTML/runtime components
      are rejected at the restricted MDX boundary; and EntityRef is canonical.
- [x] Query scope and hard constraints are enforced before ANN, graph,
      Adaptive, or rerank work; current serving does not reparse Authority
      sources; and text-only queries perform zero provider calls.
- [x] Store-wide encryption at rest is not advertised by this release line;
      the limitation is recorded in [ADR 0004](decisions/0004-at-rest-encryption.md).
- [x] GitHub Actions remain disabled by design.
