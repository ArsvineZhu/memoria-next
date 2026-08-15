# ADR 0015: Serving integration hardening

- Status: Accepted; Phase 2 integration and Phase 3 runtime benchmark/
  acceptance evidence are complete for the executed Windows matrix.
- Date: 2026-08-14

## Context

Phase 2 closes the remaining correctness gaps between the retrieval algorithms
and the production serving executor. The implementation must preserve
snapshot-pinned Derived state, make asynchronous provider work safe across
Authority generations, and make execution evidence trustworthy enough for a
release gate.

## Decisions

### Compatible semantic publication rebase

Completed semantic artifacts are published against the latest compatible
Authority generation. A tag-only, temporal-only, or Space move revision may
change the serving membership while preserving the content projection; a late
embedding from the earlier generation is therefore rebased onto the current
compatible manifest. A visible text or semantic projection change supersedes
the late result. Publication remains content-addressed and manifest-pinned.

### Provenance participates in propagation math

Tag seed provenance is not metadata-only. Activation and Diffusion initialize
and propagate seed mass using the locked weights: Explicit `1.00`, ExactSupport
`0.95`, Semantic `0.70`, Generated `0.55`, and Inherited `0.40`. Otherwise equal
paths retain the stronger provenance as stronger evidence.

### Physical profile budgets are fixed

Fast, Balanced, and Thorough use distinct bounded retrieval profiles. Balanced
uses `96 / 1024 / 3` for active tags, edge visits, and hops. Thorough uses
`192 / 4096 / 4`, a diffusion node cap of `256`, and runs independent Graph
Diffusion. These values are observable in the planner and runtime trace.

### Rust-first provider trust gate

Rust resolves the provider route and scoped Space policy before emitting work.
An external route cannot be emitted for a `local-only` Space. TypeScript's
ProviderHost remains a second egress validation layer, but policy correctness
does not depend on TypeScript rejecting already-emitted work.

### Trace is evidence, not an assertion

The production executor records executed channels and candidate counts from the
actual CandidatePool, along with bounded algorithm counters and rerank state.
The Phase 2 integration gate checks that trace counts correspond to executor
work. The later benchmark must consume this real trace rather than reproduce
retrieval logic in a simulator.

## Consequences

Late asynchronous work can complete without pinning a compatible manifest to a
stale generation, while incompatible content remains safely unpublished.
Propagation ranking is explainable by provenance and bounded by profile-level
budgets. Provider privacy is enforced before N-API work emission. The operator
trace is suitable for diagnostics and benchmark evidence, but it is not a
substitute for end-to-end runtime metrics or hard-constraint acceptance tests.

## Evidence

The integrated Phase 2 regression suite is
`crates/memoria-runtime/tests/phase2_integration.rs`. It exercises semantic
publication, generated-versus-explicit Tag ranking, runtime Diffusion,
provider work emission, and the serving trace. The same runtime-selected
quality levels are checked against the locked planner budgets. Phase 3 adds
the real runtime benchmark fixture, opt-in diagnostics, metrics, ablations,
acceptance rules, and final report. Those Phase 3 deliverables are now present
and summarized in [ADR 0013](0013-runtime-remediation-complete.md) and the
[runtime retrieval validation report](../reports/runtime-retrieval-validation.md).
