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
        return { vectors: [{ key: "u1", values: [1, 2] }] };
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
        return { vectors: [{ key: "u1", values: [1, 0, 0] }] };
      },
    },
  });
  const result = await host.execute(
    embeddingWork(3),
    new AbortController().signal,
  );
  assert.equal(result.type, "embeddings");
  if (result.type !== "embeddings") {
    throw new Error("expected an embedding result");
  }
  assert.deepEqual(result.vectors, [{ key: "u1", values: [1, 0, 0] }]);
});

test("missing vector keys are rejected before resume", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        return { vectors: [{ key: "other", values: [1, 0, 0] }] };
      },
    },
  });
  await assert.rejects(
    () => host.execute(embeddingWork(3), new AbortController().signal),
    /missing|key/i,
  );
});

test("duplicate vector keys are rejected before resume", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        return {
          vectors: [
            { key: "u1", values: [1, 0, 0] },
            { key: "u1", values: [0, 1, 0] },
          ],
        };
      },
    },
  });
  await assert.rejects(
    () =>
      host.execute(
        {
          ...embeddingWork(3),
          items: [
            { key: "u1", text: "one" },
            { key: "u2", text: "two" },
          ],
        },
        new AbortController().signal,
      ),
    /duplicate|key/i,
  );
});

test("non-finite vector values are rejected before resume", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        return {
          vectors: [{ key: "u1", values: [Number.NaN, 0, 0] }],
        };
      },
    },
  });
  await assert.rejects(
    () => host.execute(embeddingWork(3), new AbortController().signal),
    /finite|value/i,
  );
});
