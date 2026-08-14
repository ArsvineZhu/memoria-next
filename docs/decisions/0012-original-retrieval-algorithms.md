# ADR 0012: Preserve and benchmark original retrieval algorithms

## Status

Accepted for the remediation branch. Profile promotion remains evidence
dependent; retaining an operator in Thorough is a valid outcome.

## Decision

The retrieval implementation keeps the original operators as bounded,
observable channels:

- Tag Basis projection uses SVD with Fast/Balanced/Thorough caps of 12/24/48
  and a hard cap of 64;
- direct semantic ANN search is never removed when the Tag Basis residual
  channel is used;
- Activation remains best-path propagation with hop decay `0.5` and bounded
  tag, edge, and hop budgets;
- Diffusion is personalized restart diffusion and is mathematically distinct
  from Activation. It is Thorough-only until benchmark evidence promotes it;
- support, structure, and explicit Relation bonuses are clamped ranking
  features; they are not hard predicates or Authority truth;
- fusion uses rank-based reciprocal rank fusion with `k=60`, then consolidates
  revision-pinned targets and suppresses correlated resolution evidence;
- reranking is one optional provider barrier over the profile cap.

## Evidence

The deterministic corpus and query scenarios are checked in under
`benchmarks/retrieval/`. The runner emits Recall@10, MRR, nDCG@10,
duplicate-evidence rate, hard-constraint violations, latency percentiles,
provider calls, ANN searches, graph visits, diffusion iterations, and rerank
calls. Run the three profile commands locally on the current implementation;
the measured output is evidence for this branch and is not a cross-platform
performance claim.

## Public API impact

No raw operator trace is added to the default public response. The bounded
`QueryOperatorTrace` and algorithm diagnostics remain internal/test-visible,
while query semantics continue to pin Authority and Derived generations.
