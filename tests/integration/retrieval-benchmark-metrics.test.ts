import assert from "node:assert/strict";
import { test } from "node:test";

import { asSpaceId } from "../../src/index.js";
import {
  calculateQueryMetrics,
  providerCounterDelta,
  type RuntimeReturnedResult,
  type RuntimeTrace,
} from "../../benchmarks/retrieval/metrics.js";
import type { Fixture } from "../../benchmarks/retrieval/runtime-fixture.js";

const trace: RuntimeTrace = {
  channelsExecuted: ["lexical", "semantic-direct", "activation", "relation"],
  candidateCounts: [
    { channel: "lexical", count: 3 },
    { channel: "semantic-direct", count: 2 },
  ],
  activationEdgeVisits: 7,
  diffusionIterations: 0,
  relationExpansions: 2,
  rerankRequested: true,
  rerankApplied: true,
  capabilityDegraded: false,
  authorityGeneration: "9",
  correlationSuppressedEvidence: 1,
};

function fixture(): Fixture {
  return {
    dataDir: "",
    memoria: undefined as never,
    spaceIds: new Map([
      ["personal", asSpaceId("SP_personal")],
      ["work", asSpaceId("SP_work")],
    ]),
    externalIdByMemoryId: new Map(),
    documentByExternalId: new Map([
      ["a", { id: "a", spaceId: "personal", entities: ["person:alice"] }],
      ["b", { id: "b", spaceId: "work", entities: ["person:bob"] }],
      ["c", { id: "c", spaceId: "personal", entities: ["person:bob"] }],
    ]),
    counts: {
      embedding: 0,
      rerank: 0,
      enrichment: 0,
      rerankCandidateCounts: [],
    },
  } as unknown as Fixture;
}

const returned: RuntimeReturnedResult[] = [
  { id: "b", result: { spaceId: "SP_work" } },
  { id: "a", result: { spaceId: "SP_personal" } },
  { id: "c", result: { spaceId: "SP_personal" } },
];

test("Recall@10 is computed from runtime MemoryIds", () => {
  const metrics = calculateQueryMetrics({
    query: {
      id: "q1",
      case: "runtime",
      scope: ["personal"],
      entities: ["person:alice"],
      relevant: { a: 3, b: 1 },
    },
    returned,
    trace,
    providerDelta: {
      embedding: 0,
      rerank: 0,
      enrichment: 0,
      rerankCandidateCount: 0,
    },
    latencyMs: 8.125,
    fixture: fixture(),
    topK: 10,
  });

  assert.equal(metrics.recallAtK, 1);
  assert.deepEqual(metrics.top, ["b", "a", "c"]);
});

test("nDCG@10 uses labeled relevance grades", () => {
  const metrics = calculateQueryMetrics({
    query: {
      id: "q1",
      case: "graded",
      scope: ["personal"],
      relevant: { a: 3, b: 1 },
    },
    returned: [returned[0]!, returned[1]!],
    trace,
    providerDelta: {
      embedding: 0,
      rerank: 0,
      enrichment: 0,
      rerankCandidateCount: 0,
    },
    latencyMs: 1,
    fixture: fixture(),
    topK: 10,
  });

  assert(metrics.ndcgAtK > 0.7 && metrics.ndcgAtK < 0.72);
});

test("provider calls come from fixture counters", () => {
  const delta = providerCounterDelta(
    {
      embedding: 2,
      rerank: 1,
      enrichment: 4,
      rerankCandidateCounts: [3],
    },
    {
      embedding: 4,
      rerank: 2,
      enrichment: 5,
      rerankCandidateCounts: [3, 6],
    },
  );

  assert.deepEqual(delta, {
    embedding: 2,
    rerank: 1,
    enrichment: 1,
    rerankCandidateCount: 6,
  });
});

test("graph visits come from QueryOperatorTrace", () => {
  const metrics = calculateQueryMetrics({
    query: {
      id: "q1",
      case: "graph",
      scope: ["personal"],
      relevant: {},
    },
    returned: [],
    trace,
    providerDelta: {
      embedding: 0,
      rerank: 0,
      enrichment: 0,
      rerankCandidateCount: 0,
    },
    latencyMs: 1,
    fixture: fixture(),
    topK: 10,
  });

  assert.equal(metrics.graphVisits, 7);
  assert.equal(metrics.relationExpansions, 2);
});

test("hard constraint violations are computed from returned results", () => {
  const metrics = calculateQueryMetrics({
    query: {
      id: "q1",
      case: "constraints",
      scope: ["personal"],
      entities: ["person:alice"],
      relevant: {},
    },
    returned,
    trace,
    providerDelta: {
      embedding: 0,
      rerank: 0,
      enrichment: 0,
      rerankCandidateCount: 0,
    },
    latencyMs: 1,
    fixture: fixture(),
    topK: 10,
  });

  assert.equal(metrics.hardConstraintViolations, 2);
});
