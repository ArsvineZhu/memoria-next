# ADR 0005: Memoria Next incremental release baseline

## Status

Accepted as a local release baseline and regression gate. These values are
diagnostic measurements from one Windows run, not production SLOs or
cross-platform claims.

## Scope and method

The checked-in fixture is `benchmarks/incremental/cases.json`. The runner
`benchmarks/incremental/run.ts` uses the real TypeScript `createMemoria` API,
the built N-API binding, the Rust runtime compiler, and an in-process provider
counter. Each revision case opens an isolated Store, creates a baseline
Memory, waits for baseline provider work, revises the Memory, waits for the
resulting enrichment/embedding work, and runs a scoped query.

The `space-move` case invokes the Rust `memoria-derived` vector membership test
because the current public TypeScript API does not expose an Authority move
operation. That test checks the production vector index invariant: a new Space
membership reuses the immutable vector payload hash while changing the
membership scope and generation.

Command:

```text
corepack pnpm exec tsx benchmarks/incremental/run.ts
```

The runner applies projection-specific invalidation through the real runtime:
old and new source are compiled to IR, `SemanticDiff` is produced, and the
Derived invalidation plan decides whether content embedding work is queued.

## Environment

| Item                       | Observed value                                     |
| -------------------------- | -------------------------------------------------- |
| OS/architecture            | `win32/x64`                                        |
| native target              | `x86_64-pc-windows-msvc`                           |
| Node                       | `v24.19.0`                                         |
| pnpm                       | `11.20.0`                                          |
| Rust                       | `1.97.1` (`rustc 1.97.1`, edition 2024 workspace)  |
| fixture version            | `1`, 8 revision cases plus 1 vector move invariant |
| retrieval benchmark source | `ad813a9e5244ebf24da3ff04fb6b06d7d532a242`         |

The retrieval source hash identifies the checked-out benchmark fixture and
runner used by ADR 0002. The incremental run itself was executed with the
working-tree runtime change that this ADR records before its release commit.

## Incremental/provider matrix

The revision-only results were:

| edit                    | content embedding calls | enrichment calls | semantic coverage after revise | vector payload                 |
| ----------------------- | ----------------------: | ---------------: | ------------------------------ | ------------------------------ |
| formatting/comment only |                       0 |          2 total | `3`                            | reused                         |
| prose                   |                       1 |          2 total | `3`                            | replaced                       |
| Entity surface          |                       1 |          2 total | `3`                            | replaced                       |
| EntityRef only          |                       0 |          2 total | `3`                            | reused                         |
| explicit Tag only       |                       0 |          2 total | `3`                            | reused                         |
| `validTo` only          |                       0 |          2 total | `3`                            | reused                         |
| Relation                |                       0 |          2 total | `3`                            | reused                         |
| Source locator          |                       0 |          2 total | `3`                            | reused                         |
| Space move              |                       0 |   not applicable | not applicable                 | reused by Rust membership test |

The enrichment count includes the baseline and revision Base rebuild for each
revision case. It is separate from content embedding and does not weaken the
critical no-content-embedding assertions. The run observed 2 revision content
embedding calls in total, for prose and Entity surface only. All no-embedding
revision cases advanced semantic coverage to the current Authority generation
because the previous semantic payload remained valid.

## Latency, Store size, and memory

P50/P95 values below cover the eight revision cases only; the vector move test's
Cargo execution time is intentionally excluded from product startup/write
latency.

| measurement              |     P50 |     P95 | unit  |
| ------------------------ | ------: | ------: | ----- |
| open/startup             |  49.465 |  61.883 | ms    |
| revise/write             |  88.942 |  97.651 | ms    |
| scoped query             |   2.279 |   2.700 | ms    |
| Store bytes after revise | 155,978 | 156,029 | bytes |

The maximum revision-case RSS observed was 74,018,816 bytes. The runner's
all-case RSS was 74,252,288 bytes because it also runs the separate Rust move
test. Provider call totals for the complete run were 2 content embedding calls,
16 enrichment calls, and 0 rerank calls.

## Decision and consequences

Keep projection-specific invalidation as a release invariant:

- formatting/comment-only changes do not create semantic diff categories;
- visible prose and Entity surface changes may require content embedding;
- EntityRef, explicit Tag, temporal, relation, and source-locator changes do
  not require content embedding;
- a Space membership change reuses the immutable vector payload and changes
  only the scoped membership metadata.

This gate protects provider cost and data egress without claiming that every
Derived artifact is rebuilt incrementally. Base rebuild and generated-Tag work
remain independently observable. Future optimization work must preserve the
matrix and add measured cases before changing the invalidation graph.
