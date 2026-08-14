# ADR 0003: Adaptive V1 bounded recall defaults after Gate G

## Status

Accepted for the current implementation baseline. Revisit after production
feedback volume and a larger application-aligned evaluation set are available.

## Context

Adaptive V1 learns only from explicit feedback events. Its materialized state
is keyed by `SpaceId + MemoryId`, with revision evidence retained below that
target. Query ranking can use the state only after the ordinary query pipeline
has produced admissible candidates. This gate checks the failure modes that
would make feedback unsafe: rich-get-richer amplification, long-tail
starvation, negative-feedback recovery, stale-memory persistence, cross-Space
contamination, revision carry-over, and cue overfitting.

The deterministic provider-free replay fixture is:

- `benchmarks/adaptive/scenarios.jsonl` — seven named scenarios;
- `benchmarks/adaptive/replay.ts` — reducer/ranker replay and hard assertions;
- `TOP_K = 5` — diagnostic cutoff only; ranking never writes feedback.

Run commands:

```text
corepack pnpm exec tsx benchmarks/adaptive/replay.ts --adaptive=off
corepack pnpm exec tsx benchmarks/adaptive/replay.ts --adaptive=on
```

The TypeScript runner is a deterministic Gate G diagnostic and does not claim
external provider latency or replace the Rust reducer and query tests.

## V1 constants

| constant                     |                                                             value | rationale                                                                      |
| ---------------------------- | ----------------------------------------------------------------: | ------------------------------------------------------------------------------ |
| maximum adaptive prior       |                                                             `0.1` | caps the total adaptive influence and keeps base relevance primary             |
| familiarity saturation scale |                                             `4.0` positive weight | gives diminishing returns to repeated success                                  |
| accessibility half-life      |                                     `2,592,000` seconds (30 days) | decays read-time accessibility without mutating replay state                   |
| target contribution          |                                                            `0.75` | target familiarity is useful but bounded                                       |
| cue-affinity contribution    |                                                            `0.25` | tags/query class refine context without becoming a hard filter                 |
| positive weights             | `used/correct_for_query/preferred_over = 1.0`; `sufficient = 0.5` | distinguishes full and partial positive evidence                               |
| negative weight              |                                                             `1.0` | negative evidence lowers context affinity but never deletes target familiarity |

## Measurements

Both modes replayed all seven scenarios. The aggregate hard-safety results
were:

| mode         | cross-Space contamination | exact lookup exclusion caused by accessibility | unused/top-K events written | base relevance violations | maximum observed prior |
| ------------ | ------------------------: | ---------------------------------------------: | --------------------------: | ------------------------: | ---------------------: |
| adaptive off |                         0 |                                              0 |                           0 |                         0 |              0.0000000 |
| adaptive on  |                         0 |                                              0 |                           0 |                         0 |              0.0645724 |

The adaptive-on replay changed only equal-base-relevance ties: the familiar
candidate won the `rich-get-richer`, `long-tail-starvation`, and
`cross-space-contamination` tie cases, while the stronger base-relevance
candidate remained first in the long-tail and cue-overfitting cases. The stale
fixture retained the target but measured accessibility `0.3110310` after a
30-day interval. The revision carry-over scenario observed familiarity on the
new revision because the target key is Space plus Memory, while revision
evidence remains separately recorded.

The runner printed the required hard assertions in both successful runs:

```text
cross-Space contamination = 0
exact lookup exclusion caused by accessibility = 0
unused/top-K events written = 0
```

## Decision

Keep adaptive ranking enabled only as a bounded tie-breaker over already
admissible results. Do not use accessibility as a filter, do not learn from
retrieval exposure or top-K selection, and do not transfer familiarity across
Spaces. If a future feature violates any hard assertion, reduce or remove the
feature rather than weakening the invariant.

## Consequences

- Explicit feedback can make equal-quality results more useful in context.
- A stale or negatively rated target remains recoverable and inspectable; the
  system does not silently erase history during ranking.
- Scope filtering and exact evidence remain upstream safety boundaries.
- The benchmark is intentionally small and deterministic; it is a Gate G
  regression fixture, not evidence of production quality or latency.
