# ADR 0013: Runtime remediation completion

## Status

Accepted — production serving integration complete on the executed Windows
matrix. Linux and macOS remain open under platform disposition, and GitHub
Actions remain disabled by design.

Local gate status: PASS on the Windows matrix. The latest exact aggregate
`cargo test --workspace` and `corepack pnpm test` commands passed; the
TypeScript suite reported 100/100.

- Baseline reviewed commit: `605bdbb4860778051c0f6c1218044db89de46e38`
- Completion evidence commit: `0894862b309c31d1c8d077090807efaae1aa762d`
- Validation record: [runtime retrieval validation](../reports/runtime-retrieval-validation.md)

## Decision

Memoria's serving contract is implemented by a Rust-owned Physical Query
Executor. A query pins its Authority and Derived snapshot, applies scope and
hard constraints before ranking, executes the requested physical channels,
fuses and consolidates evidence, crosses a real rerank barrier when requested,
and applies Adaptive only when the compiled query uses that capability.

The TypeScript retrieval benchmark is release evidence only when it creates a
real Store, writes through Authority, waits for Derived/provider readiness,
calls public `Memoria.query()`, and measures the returned `QueryOperatorTrace`.
The checked-in benchmark now follows that path and contains no TypeScript
retrieval simulator.

## Resolved findings

| Finding                                                       | Current resolution                                                                                                                                                                                                        | Production-path evidence                                                                                           |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Original algorithms existed without serving participation     | Physical executor runs lexical, direct semantic, Tag Basis residual, Tag readout, Activation, Diffusion, Relation, fusion, support/structure, consolidation, rerank, and Recall Assessment stages behind bounded profiles | `physical_executor_e2e`, `multichannel_executor`, `production_algorithm_channels`, and the runtime benchmark trace |
| Rerank scores were discarded                                  | Rerank work is built from actual consolidated candidates, stored across the query barrier, validated, and applied once                                                                                                    | `rerank_barrier`, `physical_executor_regressions`, runtime `rerankApplied` trace                                   |
| Required semantic wait degraded to lexical                    | Required capability remains a hard contract; wait ends in readiness or `CAPABILITY_NOT_READY`                                                                                                                             | `capability_wait`, `physical_executor_regressions`                                                                 |
| Semantic replaced lexical                                     | Explicit semantic execution retains the lexical channel and fuses both rank lists                                                                                                                                         | `multichannel_executor`, `physical_executor_e2e`                                                                   |
| Semantic publication lost after a compatible generation moved | Compatible immutable artifacts rebase to the latest serving generation; changed projections supersede late work                                                                                                           | `semantic_rebase`, `phase2_integration`                                                                            |
| Adaptive was applied unconditionally or replayed per query    | Adaptive is capability-gated and reads a snapshot-pinned materialized state                                                                                                                                               | `adaptive_query_gate`, `adaptive_restart`                                                                          |
| Provenance weights were metadata-only                         | Activation and Diffusion seed mass uses the locked Explicit, ExactSupport, Semantic, Generated, and Inherited weights                                                                                                     | `provenance_weighting`, `phase2_integration`                                                                       |
| Thorough budget did not match the locked profile              | Thorough uses 192 active tags, 4096 edge visits, 4 hops, and independent Diffusion                                                                                                                                        | `retrieval_profiles`, `phase2_integration`                                                                         |
| Rust provider trust gate was incomplete                       | Rust resolves route trust and Space policy before emitting external work; TypeScript remains a second egress gate                                                                                                         | `provider_trust_gate`, `phase2_integration`                                                                        |
| Benchmark simulated retrieval outside Runtime                 | Fixture, fake providers, public query, diagnostics trace, metrics, ablations, and acceptance all run through the real Runtime                                                                                             | `benchmark-runtime-path.test.ts`, `runtime-ablation.test.ts`, and `runtime-retrieval-validation.md`                |

## Production-path evidence

The focused serving evidence is covered by these Rust suites:

- `cargo test -p memoria-runtime --test physical_executor_e2e`
- `cargo test -p memoria-runtime --test physical_executor_regressions`
- `cargo test -p memoria-runtime --test multichannel_executor`
- `cargo test -p memoria-runtime --test phase2_integration`
- `cargo test -p memoria-runtime --test rerank_barrier`
- `cargo test -p memoria-runtime --test capability_wait`
- `cargo test -p memoria-runtime --test adaptive_query_gate`
- `cargo test -p memoria-runtime --test provider_trust_gate`
- `cargo test -p memoria-runtime --test semantic_rebase`

The public and benchmark evidence is covered by:

- `tests/public/query-diagnostics.test.ts`
- `tests/integration/benchmark-runtime-path.test.ts`
- `tests/integration/retrieval-benchmark-metrics.test.ts`
- `tests/integration/runtime-ablation.test.ts`
- `tests/integration/retrieval-acceptance.test.ts`
- `tests/integration/remediation-baseline.test.ts`

The release commands are:

```text
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile fast
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile thorough
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile balanced --ablation all
corepack pnpm exec tsx benchmarks/retrieval/run.ts --evaluate-acceptance
```

The generated report paths and hashes from the executed run are recorded in
[runtime retrieval validation](../reports/runtime-retrieval-validation.md).

## Algorithm preservation

The following original algorithm surfaces remain implemented and are proven
through the real executor rather than only by isolated module tests:

- Tag Basis projection and semantic-residual retrieval;
- direct semantic retrieval as an independent channel;
- scoped Tag readout and provenance-weighted Activation;
- independent normalized Graph Diffusion;
- explicit Relation expansion;
- RRF fusion with `k = 60`;
- support/structure bonuses and correlation suppression;
- hierarchical evidence consolidation;
- real provider reranking after consolidation;
- Recall, accessibility, and effort assessment;
- capability-gated Adaptive ordering from the materialized snapshot.

## Profile disposition

The acceptance runner compares every candidate against the same eight-query
fixture set. It applies these exact promotion rules:

```text
hard-constraint violations == 0
overall nDCG@10 decrease <= 0.005 absolute
target Recall@10 improvement >= 0.05 absolute
P95 latency increase <= 25%
no new provider call without the corresponding requested capability
```

The executed deterministic local-provider run promoted no advanced profile to
Balanced. The automatically selected Balanced default remains provider-free
lexical; advanced semantic, associative, rerank, and Adaptive capabilities
remain explicit, and Diffusion remains Thorough-only. This is a measured
profile decision, not a claim that the algorithms are absent or incorrect.

## Known limitations

- The measured provider is deterministic and local; its latency and ranking
  values are not external-provider or production-scale claims.
- The benchmark fixture is intentionally small and is a regression/proof
  corpus, not a substitute for a larger relevance evaluation.
- The executed native matrix is Windows `win32-x64-msvc`; Linux and macOS
  still require their native local matrices.
- Benchmark result files are ignored machine artifacts. Their exact paths and
  SHA-256 values are recorded for this validation run, but they are not
  committed as package data.
- An earlier default parallel Rust workspace run encountered transient Windows
  OS error 33 while temporary Derived/Tantivy files were created or removed;
  the latest exact default rerun passed. The serialized workspace gate remains
  a reproducible fallback, and the transient file-lock event is not a serving
  assertion or product error.
