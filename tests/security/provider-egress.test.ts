import assert from "node:assert/strict";
import test from "node:test";

import {
  ProviderHost,
  createProviderEgressGuard,
} from "../../src/providers/host.js";
import type { EmbeddingProvider } from "../../src/providers/types.js";

test("required semantic fails before local-only provider egress", async () => {
  let calls = 0;
  const embedding: EmbeddingProvider = {
    async execute() {
      calls += 1;
      return { vectors: [{ key: "memory-1", values: [1, 0] }] };
    },
  };
  const host = new ProviderHost({
    providers: { embedding },
    onDataEgress: createProviderEgressGuard({
      embedding: false,
      rerank: false,
      enrichment: false,
    }),
  });

  await assert.rejects(
    () =>
      host.execute(
        {
          type: "embedding",
          workId: "WORK_1",
          signature: "local-only",
          dimensions: 2,
          items: [{ key: "memory-1", text: "private source" }],
        },
        new AbortController().signal,
      ),
    (error: unknown) =>
      error instanceof Error && error.message.includes("CAPABILITY_NOT_READY"),
  );
  assert.equal(calls, 0);
});
