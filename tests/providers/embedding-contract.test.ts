import assert from "node:assert/strict";
import { test } from "node:test";

import { ProviderHost } from "../../src/providers/host.js";

function embeddingWork(dimensions: number) {
  return {
    type: "embedding" as const,
    workId: "W-embedding-1",
    signature: "embedding-v1",
    dimensions,
    items: [{ key: "u1", text: "career" }],
  };
}

test("wrong embedding dimension is rejected before resume", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        return [[1, 2]];
      },
    },
  });
  await assert.rejects(
    () => host.execute(embeddingWork(3), new AbortController().signal),
    /dimension/i,
  );
});

test("embedding payload is normalized with stable item keys", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        return { vectors: [[1, 0, 0]] };
      },
    },
  });
  const result = await host.execute(
    embeddingWork(3),
    new AbortController().signal,
  );
  assert.deepEqual(result.embeddings, [{ key: "u1", values: [1, 0, 0] }]);
});
