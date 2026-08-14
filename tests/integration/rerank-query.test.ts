import assert from "node:assert/strict";
import { test } from "node:test";

import { ProviderHost } from "../../src/providers/host.js";
import { rerankScopedCandidates } from "../../src/retrieval/rerank.js";

test("reranker cannot restore out-of-scope result", async () => {
  const spaceA = "SP_A";
  const spaceB = "SP_B";
  const observed: string[][] = [];
  const provider = new ProviderHost({
    rerank: {
      trust: "external",
      async execute(work) {
        observed.push([...work.candidates]);
        return work.candidates.map((handle, index) => ({
          handle,
          score: index === 0 ? 0.1 : 1.0,
        }));
      },
    },
  });

  const results = await rerankScopedCandidates({
    query: "career",
    scope: [spaceA],
    candidates: [
      { handle: "A-1", spaceId: spaceA, text: "career in A", score: 0.4 },
      { handle: "B-1", spaceId: spaceB, text: "career in B", score: 0.9 },
    ],
    provider,
  });

  assert.deepEqual(observed, [["A-1"]]);
  assert(results.every((candidate) => candidate.spaceId === spaceA));
  assert.deepEqual(
    results.map((candidate) => candidate.handle),
    ["A-1"],
  );
});
