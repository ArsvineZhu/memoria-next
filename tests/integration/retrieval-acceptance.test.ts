import assert from "node:assert/strict";
import { test } from "node:test";

import {
  evaluateAcceptance,
  type AcceptanceInput,
} from "../../benchmarks/retrieval/acceptance.js";

function input(overrides: Partial<AcceptanceInput> = {}): AcceptanceInput {
  return {
    baseline: {
      recallAt10: 0.8,
      ndcgAt10: 0.8,
      p95LatencyMs: 100,
      embeddingProviderCalls: 0,
      rerankProviderCalls: 0,
      enrichmentProviderCalls: 0,
    },
    candidate: {
      hardConstraintViolationCount: 0,
      recallAt10: 0.85,
      ndcgAt10: 0.8,
      p95LatencyMs: 110,
      embeddingProviderCalls: 0,
      rerankProviderCalls: 0,
      enrichmentProviderCalls: 0,
    },
    requestedCapabilities: [],
    ...overrides,
  };
}

test("hard constraint violation always fails acceptance", () => {
  const result = evaluateAcceptance(
    input({
      candidate: {
        ...input().candidate,
        hardConstraintViolationCount: 1,
      },
    }),
  );

  assert.equal(result.accepted, false);
  assert(result.failures.includes("hardConstraints"));
});

test("operator promotion requires target Recall@10 +5pp", () => {
  const result = evaluateAcceptance(input({ targetRecallImprovement: 0.049 }));

  assert.equal(result.accepted, false);
  assert(result.failures.includes("recall"));
});

test("overall nDCG@10 may not drop by more than 0.5pp", () => {
  const result = evaluateAcceptance(
    input({
      candidate: {
        ...input().candidate,
        ndcgAt10: 0.794,
      },
    }),
  );

  assert.equal(result.accepted, false);
  assert(result.failures.includes("ndcg"));
});

test("P95 latency increase above 25 percent fails promotion", () => {
  const result = evaluateAcceptance(
    input({
      candidate: {
        ...input().candidate,
        p95LatencyMs: 126,
      },
    }),
  );

  assert.equal(result.accepted, false);
  assert(result.failures.includes("latency"));
});

test("provider calls may not appear when capability was not requested", () => {
  const result = evaluateAcceptance(
    input({
      candidate: {
        ...input().candidate,
        embeddingProviderCalls: 1,
        rerankProviderCalls: 1,
        enrichmentProviderCalls: 1,
      },
    }),
  );

  assert.equal(result.accepted, false);
  assert(result.failures.includes("providerCalls"));
});
