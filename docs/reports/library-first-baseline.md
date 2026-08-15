# Library-First Baseline

Reviewed commit: `500580947c830d73373d7a99eb722431f74da5d6`

This report classifies source modules before library substitution. `REVIEW_REQUIRED` is a hard failure for the architecture inventory gate.

## Summary

| Metric | Value |
| --- | ---: |
| Source files | 150 |
| Non-empty LOC | 33445 |
| Direct dependency count | 32 |
| Self-authored infrastructure LOC | 6197 |
| Infrastructure LOC percentage | 18.53% |
| Physical retrieval infrastructure LOC | 3750 |

| Class | Files | LOC |
| --- | ---: | ---: |
| DOMAIN | 53 | 9734 |
| ALGORITHM | 16 | 2731 |
| INTEGRATION | 61 | 14783 |
| INFRASTRUCTURE | 20 | 6197 |
| REVIEW_REQUIRED | 0 | 0 |

## Custom persistent markers

- `tombstone`: `crates/memoria-derived/src/catalog.rs`, `crates/memoria-derived/src/gc.rs`, `crates/memoria-derived/src/lib.rs`, `crates/memoria-derived/src/vector.rs`, `crates/memoria-runtime/src/engine.rs`
- `segment`: `crates/memoria-derived/src/catalog.rs`, `crates/memoria-derived/src/compiler.rs`, `crates/memoria-derived/src/gc.rs`, `crates/memoria-derived/src/lib.rs`, `crates/memoria-derived/src/projection/embedding_view.rs`, `crates/memoria-derived/src/vector.rs`, `crates/memoria-runtime/src/engine.rs`
- `compaction`: `crates/memoria-derived/src/lib.rs`, `crates/memoria-derived/src/vector.rs`
- `MEMVEC01`: `crates/memoria-derived/src/vector.rs`

## Custom lifecycle/state-machine files

- `crates/memoria-derived/src/gc.rs`
- `crates/memoria-derived/src/lease.rs`
- `crates/memoria-derived/src/scheduler.rs`

## Candidate ecosystem replacements

| Concern | Candidate |
| --- | --- | 
| Markdown/MDX syntax foundation | markdown = 1.0.0 |
| SQLite schema migration | rusqlite_migration = 2.6.0 |
| atomic source-object writes | atomic-write-file = 0.3.0 |
| TypeScript provider retry | p-retry = 8.0.0 |
| Rust transient retry | backon = 1.6.0 |
| generic Rust diagnostics | tracing = 0.1.44 |
| association graph container | petgraph = 0.8.3 |
| process cache eviction and TTL | moka = 0.12.15 |
| physical lexical/vector retrieval | lancedb = 0.33.0 (POC-gated) |
| durable build-job queue | apalis = 0.7.4 (POC-gated) |

## Module inventory

| Path | Class | LOC | Reason |
| --- | --- | ---: | --- | 
| `benchmarks/adaptive/replay.ts` | INTEGRATION | 440 | Library-first tooling and benchmark integration |
| `benchmarks/encryption/measure.ts` | INTEGRATION | 274 | Library-first tooling and benchmark integration |
| `benchmarks/incremental/run.ts` | INTEGRATION | 327 | Library-first tooling and benchmark integration |
| `benchmarks/retrieval/acceptance.ts` | INTEGRATION | 120 | Library-first tooling and benchmark integration |
| `benchmarks/retrieval/fake-providers.ts` | INTEGRATION | 95 | Library-first tooling and benchmark integration |
| `benchmarks/retrieval/metrics.ts` | INTEGRATION | 304 | Library-first tooling and benchmark integration |
| `benchmarks/retrieval/report.ts` | INTEGRATION | 84 | Library-first tooling and benchmark integration |
| `benchmarks/retrieval/run.ts` | INTEGRATION | 509 | Library-first tooling and benchmark integration |
| `benchmarks/retrieval/runtime-fixture.ts` | INTEGRATION | 254 | Library-first tooling and benchmark integration |
| `crates/memoria-adaptive/src/checkpoint.rs` | INFRASTRUCTURE | 30 | Adaptive persistence and checkpoint mechanics |
| `crates/memoria-adaptive/src/event.rs` | DOMAIN | 164 | Adaptive event and feedback semantics |
| `crates/memoria-adaptive/src/lib.rs` | INFRASTRUCTURE | 19 | Adaptive persistence and checkpoint mechanics |
| `crates/memoria-adaptive/src/log.rs` | INFRASTRUCTURE | 700 | Adaptive persistence and checkpoint mechanics |
| `crates/memoria-adaptive/src/model.rs` | DOMAIN | 214 | Adaptive event and feedback semantics |
| `crates/memoria-adaptive/src/purge.rs` | DOMAIN | 15 | Adaptive event and feedback semantics |
| `crates/memoria-adaptive/src/reducer.rs` | DOMAIN | 66 | Adaptive event and feedback semantics |
| `crates/memoria-adaptive/src/reset.rs` | INFRASTRUCTURE | 29 | Adaptive persistence and checkpoint mechanics |
| `crates/memoria-authority/src/cas.rs` | INFRASTRUCTURE | 74 | Authority storage and file mechanics |
| `crates/memoria-authority/src/db.rs` | INFRASTRUCTURE | 324 | Authority storage and file mechanics |
| `crates/memoria-authority/src/integrity.rs` | INFRASTRUCTURE | 701 | Authority storage and file mechanics |
| `crates/memoria-authority/src/layout.rs` | INFRASTRUCTURE | 171 | Authority storage and file mechanics |
| `crates/memoria-authority/src/lib.rs` | INFRASTRUCTURE | 26 | Authority storage and file mechanics |
| `crates/memoria-authority/src/lock.rs` | INFRASTRUCTURE | 45 | Authority storage and file mechanics |
| `crates/memoria-authority/src/model.rs` | DOMAIN | 608 | Authority lifecycle and mutation semantics |
| `crates/memoria-authority/src/mutation.rs` | DOMAIN | 1872 | Authority lifecycle and mutation semantics |
| `crates/memoria-authority/src/purge.rs` | DOMAIN | 296 | Authority lifecycle and mutation semantics |
| `crates/memoria-authority/src/read.rs` | DOMAIN | 482 | Authority lifecycle and mutation semantics |
| `crates/memoria-authority/src/schema.rs` | INFRASTRUCTURE | 297 | Authority storage and file mechanics |
| `crates/memoria-derived/src/artifact.rs` | INFRASTRUCTURE | 163 | Custom Derived physical artifact infrastructure |
| `crates/memoria-derived/src/catalog.rs` | INFRASTRUCTURE | 1525 | Custom Derived physical artifact infrastructure |
| `crates/memoria-derived/src/compiler.rs` | INTEGRATION | 258 | Derived projection and provider integration |
| `crates/memoria-derived/src/dependency.rs` | INTEGRATION | 199 | Derived projection and provider integration |
| `crates/memoria-derived/src/embedding.rs` | INTEGRATION | 283 | Derived projection and provider integration |
| `crates/memoria-derived/src/enrichment.rs` | INTEGRATION | 272 | Derived projection and provider integration |
| `crates/memoria-derived/src/gc.rs` | INFRASTRUCTURE | 134 | Custom Derived physical artifact infrastructure |
| `crates/memoria-derived/src/lease.rs` | INFRASTRUCTURE | 48 | Custom Derived physical artifact infrastructure |
| `crates/memoria-derived/src/lexical_artifact.rs` | INFRASTRUCTURE | 369 | Custom Derived physical artifact infrastructure |
| `crates/memoria-derived/src/lib.rs` | INTEGRATION | 142 | Derived projection and provider integration |
| `crates/memoria-derived/src/manifest.rs` | DOMAIN | 123 | Derived snapshot and lifecycle domain metadata |
| `crates/memoria-derived/src/projection/embedding_view.rs` | INTEGRATION | 175 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/entities.rs` | INTEGRATION | 68 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/lexical.rs` | INTEGRATION | 197 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/mod.rs` | INTEGRATION | 17 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/relations.rs` | INTEGRATION | 74 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/rerank_view.rs` | INTEGRATION | 83 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/serving.rs` | INTEGRATION | 79 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/structural.rs` | INTEGRATION | 61 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/tags.rs` | INTEGRATION | 83 | Derived projection and provider integration |
| `crates/memoria-derived/src/projection/temporal.rs` | INTEGRATION | 58 | Derived projection and provider integration |
| `crates/memoria-derived/src/scheduler.rs` | INFRASTRUCTURE | 209 | Custom Derived physical artifact infrastructure |
| `crates/memoria-derived/src/status.rs` | DOMAIN | 29 | Derived snapshot and lifecycle domain metadata |
| `crates/memoria-derived/src/tag_dictionary.rs` | INTEGRATION | 139 | Derived projection and provider integration |
| `crates/memoria-derived/src/tag_graph.rs` | INTEGRATION | 308 | Derived projection and provider integration |
| `crates/memoria-derived/src/vector.rs` | INFRASTRUCTURE | 1162 | Custom Derived physical artifact infrastructure |
| `crates/memoria-mdx/src/canonical.rs` | DOMAIN | 215 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/diff.rs` | DOMAIN | 265 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/ir.rs` | DOMAIN | 185 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/lib.rs` | DOMAIN | 33 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/lint.rs` | DOMAIN | 83 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/mdast_adapter.rs` | DOMAIN | 389 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/patch.rs` | DOMAIN | 516 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/profile.rs` | DOMAIN | 195 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/source.rs` | DOMAIN | 85 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/syntax.rs` | DOMAIN | 53 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/test_support.rs` | DOMAIN | 19 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/time.rs` | DOMAIN | 197 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-mdx/src/validate.rs` | DOMAIN | 353 | Restricted MDX and canonical Memory semantics |
| `crates/memoria-napi/src/convert.rs` | INTEGRATION | 1205 | N-API integration boundary |
| `crates/memoria-napi/src/error.rs` | INTEGRATION | 7 | N-API integration boundary |
| `crates/memoria-napi/src/lib.rs` | INTEGRATION | 346 | N-API integration boundary |
| `crates/memoria-query/src/adaptive.rs` | ALGORITHM | 88 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/algorithms/activation.rs` | ALGORITHM | 193 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/algorithms/diffusion.rs` | ALGORITHM | 212 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/algorithms/mod.rs` | ALGORITHM | 18 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/algorithms/structure.rs` | ALGORITHM | 57 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/algorithms/support.rs` | ALGORITHM | 71 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/algorithms/tag_basis.rs` | ALGORITHM | 216 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/assessment.rs` | DOMAIN | 86 | Structured query and retrieval contract types |
| `crates/memoria-query/src/association.rs` | INFRASTRUCTURE | 140 | Custom query graph container |
| `crates/memoria-query/src/compile.rs` | INTEGRATION | 59 | Query planning and execution integration |
| `crates/memoria-query/src/consolidate.rs` | INTEGRATION | 222 | Query planning and execution integration |
| `crates/memoria-query/src/continuation.rs` | INTEGRATION | 115 | Query planning and execution integration |
| `crates/memoria-query/src/evidence.rs` | DOMAIN | 71 | Structured query and retrieval contract types |
| `crates/memoria-query/src/exact.rs` | ALGORITHM | 335 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/executor.rs` | INTEGRATION | 529 | Query planning and execution integration |
| `crates/memoria-query/src/fusion.rs` | ALGORITHM | 268 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/history.rs` | ALGORITHM | 103 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/lexical.rs` | ALGORITHM | 131 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/lexical_operator.rs` | INTEGRATION | 11 | Query planning and execution integration |
| `crates/memoria-query/src/lib.rs` | INTEGRATION | 91 | Query planning and execution integration |
| `crates/memoria-query/src/model.rs` | DOMAIN | 348 | Structured query and retrieval contract types |
| `crates/memoria-query/src/planner.rs` | INTEGRATION | 184 | Query planning and execution integration |
| `crates/memoria-query/src/relation_expand.rs` | ALGORITHM | 172 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/rerank.rs` | ALGORITHM | 194 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/response.rs` | DOMAIN | 42 | Structured query and retrieval contract types |
| `crates/memoria-query/src/semantic.rs` | ALGORITHM | 285 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/snapshot.rs` | DOMAIN | 17 | Structured query and retrieval contract types |
| `crates/memoria-query/src/tags.rs` | ALGORITHM | 283 | Original query algorithms and ranking semantics |
| `crates/memoria-query/src/trace.rs` | DOMAIN | 76 | Structured query and retrieval contract types |
| `crates/memoria-query/src/validate.rs` | DOMAIN | 115 | Structured query and retrieval contract types |
| `crates/memoria-runtime/src/backup.rs` | INTEGRATION | 440 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/diagnostics.rs` | INFRASTRUCTURE | 31 | Generic runtime diagnostics plumbing |
| `crates/memoria-runtime/src/discovery.rs` | INTEGRATION | 54 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/engine.rs` | INTEGRATION | 2965 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/lib.rs` | INTEGRATION | 47 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/limits.rs` | INTEGRATION | 46 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/privacy.rs` | INTEGRATION | 243 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/provider.rs` | INTEGRATION | 246 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/purge.rs` | INTEGRATION | 104 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/query_operation.rs` | INTEGRATION | 297 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/receipt.rs` | INTEGRATION | 155 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/status.rs` | INTEGRATION | 11 | Runtime and provider boundary integration |
| `crates/memoria-runtime/src/transfer.rs` | INTEGRATION | 137 | Runtime and provider boundary integration |
| `crates/memoria-types/src/digest.rs` | DOMAIN | 93 | Memoria type and identity semantics |
| `crates/memoria-types/src/entity_ref.rs` | DOMAIN | 96 | Memoria type and identity semantics |
| `crates/memoria-types/src/error.rs` | DOMAIN | 92 | Memoria type and identity semantics |
| `crates/memoria-types/src/id.rs` | DOMAIN | 190 | Memoria type and identity semantics |
| `crates/memoria-types/src/lib.rs` | DOMAIN | 31 | Memoria type and identity semantics |
| `crates/memoria-types/src/snapshot.rs` | DOMAIN | 158 | Memoria type and identity semantics |
| `crates/memoria-types/src/time.rs` | DOMAIN | 87 | Memoria type and identity semantics |
| `scripts/library-first/inventory.mjs` | INTEGRATION | 295 | Library-first tooling and benchmark integration |
| `scripts/library-first/verify-policy.mjs` | INTEGRATION | 154 | Library-first tooling and benchmark integration |
| `scripts/run-tests.mjs` | INTEGRATION | 31 | Library-first tooling and benchmark integration |
| `scripts/verify-docs.mjs` | INTEGRATION | 192 | Library-first tooling and benchmark integration |
| `scripts/verify-memory-skill.mjs` | INTEGRATION | 35 | Library-first tooling and benchmark integration |
| `scripts/verify-packed-consumer.mjs` | INTEGRATION | 118 | Library-first tooling and benchmark integration |
| `src/admin/governance.ts` | DOMAIN | 398 | Public authoring, agent, administration, and domain API |
| `src/admin/index.ts` | DOMAIN | 72 | Public authoring, agent, administration, and domain API |
| `src/agent/identity.ts` | DOMAIN | 186 | Public authoring, agent, administration, and domain API |
| `src/agent/index.ts` | DOMAIN | 37 | Public authoring, agent, administration, and domain API |
| `src/agent/memory-owner.ts` | DOMAIN | 118 | Public authoring, agent, administration, and domain API |
| `src/agent/tools.ts` | DOMAIN | 279 | Public authoring, agent, administration, and domain API |
| `src/authoring/index.ts` | DOMAIN | 5 | Public authoring, agent, administration, and domain API |
| `src/authoring/serialize.ts` | DOMAIN | 25 | Public authoring, agent, administration, and domain API |
| `src/domain/config.ts` | DOMAIN | 106 | Public authoring, agent, administration, and domain API |
| `src/domain/documents.ts` | DOMAIN | 92 | Public authoring, agent, administration, and domain API |
| `src/domain/errors.ts` | DOMAIN | 93 | Public authoring, agent, administration, and domain API |
| `src/domain/feedback.ts` | DOMAIN | 106 | Public authoring, agent, administration, and domain API |
| `src/domain/ids.ts` | DOMAIN | 15 | Public authoring, agent, administration, and domain API |
| `src/domain/query.ts` | DOMAIN | 201 | Public authoring, agent, administration, and domain API |
| `src/domain/spaces.ts` | DOMAIN | 40 | Public authoring, agent, administration, and domain API |
| `src/domain/status.ts` | DOMAIN | 2 | Public authoring, agent, administration, and domain API |
| `src/engine/create-memoria.ts` | INTEGRATION | 52 | TypeScript native and provider integration |
| `src/engine/memoria.ts` | INTEGRATION | 552 | TypeScript native and provider integration |
| `src/index.ts` | INTEGRATION | 43 | TypeScript native and provider integration |
| `src/native/binding.ts` | INTEGRATION | 17 | TypeScript native and provider integration |
| `src/native/protocol.ts` | INTEGRATION | 423 | TypeScript native and provider integration |
| `src/providers/host.ts` | INTEGRATION | 304 | TypeScript native and provider integration |
| `src/providers/types.ts` | INTEGRATION | 150 | TypeScript native and provider integration |
| `src/retrieval/rerank.ts` | ALGORITHM | 105 | TypeScript retrieval integration algorithm |
