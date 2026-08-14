# ADR 0014: Physical Query Executor

## Status

Accepted.

## Context

The remediation work added the retrieval algorithms and their unit-level
coverage, but the production query path still selected one of Exact, Lexical,
or Semantic execution. That left the serving behavior different from the
algorithm and benchmark evidence. It also allowed rerank scores to be
discarded, allowed Adaptive state to affect queries that did not request it,
and treated a required-but-not-ready capability as an implicit lexical
fallback.

## Decision

Memoria uses a Rust-owned, typed Physical Query Executor. Query compilation
pins the Authority generation, Derived Manifest, and any requested Adaptive
snapshot. The compiled plan is carried through a staged query operation so
provider work can pause and resume without rebuilding the query from mutable
state.

The serving pipeline is ordered as follows:

1. Pin the query snapshot and validate scope, lifecycle, history, Entity, Tag,
   and Memory constraints.
2. Read current serving records and the manifest-pinned lexical artifact.
3. Execute Exact and Lexical independently whenever their query inputs
   require them.
4. When Semantic is used, execute the direct ANN channel and the optional Tag
   Basis residual ANN channel in parallel. The direct channel is never
   replaced by the residual channel.
5. When Associative is used, execute scoped Tag readout, Activation, optional
   Thorough-profile Diffusion, and explicit in-scope Relation expansion.
6. Fuse channel rank lists with RRF `k = 60`, merge evidence by one
   `CandidateKey`, suppress correlated support, and apply bounded support,
   structure, and relation bonuses.
7. Consolidate physical evidence into one public Memory result and, when
   requested, pause at a provider rerank barrier built from those actual
   consolidated results.
8. Apply Adaptive ordering only when the compiled capability execution uses
   Adaptive, using the snapshot captured at query start.
9. Assess recall and accessibility after the final ordering and persist the
   retrieval receipt.

The following capability rules are part of the contract:

- A text cue does not infer Semantic, Associative, Reranking, or Adaptive.
- A required capability remains required. `onNotReady: wait` creates a
  readiness operation and either observes readiness before its deadline or
  returns the capability error; it does not silently remove the requirement.
- A preferred capability may use the documented degraded path when its
  provider or artifact boundary is unavailable.
- Space scope and hard constraints are applied before every channel can
  contribute a candidate, before fusion, and before reranking.
- Provider work is typed and remains behind the Rust/TypeScript protocol
  boundary. Rerank scores are validated, stored on the operation, applied once,
  and reflected by `rerank_applied` in the operator trace.

Derived serving records persist explicit relation labels and structured
`MemoryRef` targets. Runtime creates Relation links only from those persisted
Authority-derived references and only when the target remains in the pinned
query scope; it never infers a relation from co-occurrence, embeddings, or
provider output.

## Invariants

- Every physical channel contributes ranked evidence, not a raw score summed
  with another channel's score.
- A target has one fused candidate key; independent evidence remains attached
  to that candidate.
- Candidate count limits are applied after scope filtering and fusion, while
  evidence budgets are applied during consolidation.
- Correlated parent/child or repeated evidence cannot manufacture independent
  support.
- Relation expansion is bounded by the quality profile and cannot cross the
  authorized Space set.
- Adaptive is a bounded post-retrieval tie-breaker. It cannot restore a
  hard-filtered candidate or add a candidate that the base executor did not
  retrieve.
- The operator trace records executed channels, candidate counts, algorithm
  diagnostics, fusion suppression, readiness degradation, and rerank request
  versus application.

## Persistence boundaries

The lexical operator opens the immutable lexical artifact referenced by the
query's Derived Manifest. Current queries do not reparse Authority source
bytes. Semantic vectors and ANN segments remain manifest-pinned Derived
artifacts. Adaptive reads use the durable materialized snapshot and pin a clone
across provider barriers; query execution does not replay the complete event
log.

## Evidence

The production-path gate is `crates/memoria-runtime/tests/physical_executor_e2e.rs`.
It covers lexical-only serving, lexical plus Semantic direct retrieval, Tag
Basis residuals, Activation, Thorough Diffusion, explicit Relation expansion,
real post-fusion reranking, inactive and active Adaptive behavior, required
Semantic readiness waiting, and Entity/Space hard constraints across channels.
The TypeScript regression suite additionally verifies that the public text-only
query contract does not infer Semantic capability and that readiness pending
continues through the native operation protocol.

ADR 0013 remains incomplete until the Phase 2 publication/provider fixes and
the Phase 3 real-runtime benchmark provide release evidence. This ADR records
the serving architecture and its Phase 1 proof; it does not turn a module or a
simulated benchmark into completion evidence.
