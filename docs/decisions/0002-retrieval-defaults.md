# ADR 0002: Retrieval defaults after Gate F

## Status

Accepted for the current implementation baseline. Revisit after a larger
application-aligned corpus and production latency samples are available.

## Context

Gate F evaluates the retrieval profiles implemented in the semantic/tag
retrieval plan. The checked-in fixture contains 12 documents and 8 labeled
queries covering current and historical revisions, entity constraints, Tag
association, explicit relations, and cross-Space scope boundaries. The runner
reports Recall@5, MRR, nDCG@5, duplicate evidence, latency, provider calls,
bounded graph visits, working-set bytes, and Tag Basis quality diagnostics.

Run command:

```text
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile lexical
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile lexical+semantic
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile tag-association
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile tag-basis-residual
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile activation
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile diffusion
corepack pnpm exec tsx benchmarks/retrieval/run.ts --profile rerank
```

The scorer is deterministic and provider-free; `providerCalls` is the modeled
single rerank barrier per non-empty query, not a claim about external network
latency. Latency is a local wall-clock diagnostic and is not used as the sole
default-selection criterion.

## Measurements

The final sequential `--profile=all` run produced the following summary. The
working set was 4,770 bytes for every profile and duplicate evidence was zero.

| profile            | Recall@5 |    MRR | nDCG@5 | provider calls | graph visits | basis rank mean | basis skips |
| ------------------ | -------: | -----: | -----: | -------------: | -----------: | --------------: | ----------: |
| lexical            |   1.0000 | 1.0000 | 0.9532 |              0 |            0 |               0 |           0 |
| lexical+semantic   |   1.0000 | 0.9375 | 0.9387 |              0 |            0 |               0 |           0 |
| tag-association    |   1.0000 | 0.9375 | 0.9387 |              0 |            0 |               0 |           0 |
| tag-basis-residual |   1.0000 | 0.9375 | 0.9387 |              0 |            0 |           0.625 |           4 |
| activation         |   1.0000 | 0.9375 | 0.9387 |              0 |           47 |               0 |           0 |
| diffusion          |   1.0000 | 0.9375 | 0.9387 |              0 |           48 |               0 |           0 |
| rerank             |   1.0000 | 0.9375 | 0.9387 |              8 |            0 |               0 |           0 |

## Decision

Keep `lexical` as the balanced default for this release line. It is the only
profile that improves the measured ranking metrics over the other tested
profiles, while requiring no provider call and no graph traversal.

Keep semantic, Tag association, Tag Basis/residual, activation, diffusion, and
rerank available as explicit experimental operators. They do not become
planner defaults until a broader corpus demonstrates a quality improvement
without violating provider, graph, and latency budgets. The benchmark's
cross-Space cases also confirm that scope filtering occurs before advanced
ranking; out-of-scope documents are not eligible for recovery by reranking.

## Consequences

- Default query execution remains deterministic and local.
- Advanced retrieval code is covered by focused tests but does not silently
  increase cost or data egress for default callers.
- Future ablations must retain the same labeled cases and add hard negatives
  before changing this decision.
