import assert from "node:assert/strict";
import { test } from "node:test";

import { ProviderHost } from "../../src/providers/host.js";
import { ProviderExecutionError } from "../../src/providers/types.js";

test("provider host preserves work id", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        return [[1, 0, 0]];
      },
    },
  });
  const result = await host.execute(
    {
      type: "embedding",
      workId: "W1",
      signature: "embedding-v1",
      dimensions: 3,
      items: [{ key: "u1", text: "career" }],
    },
    new AbortController().signal,
  );
  assert.equal(result.workId, "W1");
});

test("provider host retries provider-specific failures and applies egress policy", async () => {
  let attempts = 0;
  let sentText = "";
  const host = new ProviderHost({
    providers: {
      embedding: {
        async execute(work) {
          attempts += 1;
          sentText = work.items[0]?.text ?? "";
          if (attempts === 1) {
            throw new Error("transient network failure");
          }
          return [[1, 0, 0]];
        },
      },
    },
    maxAttempts: 2,
    onDataEgress(work) {
      if (work.type !== "embedding") {
        return work;
      }
      return {
        ...work,
        items: work.items.map((item) => ({ ...item, text: "redacted" })),
      };
    },
  });

  const result = await host.execute(
    {
      type: "embedding",
      workId: "W2",
      signature: "embedding-v1",
      dimensions: 3,
      items: [{ key: "u2", text: "private career note" }],
    },
    new AbortController().signal,
  );
  assert.equal(result.workId, "W2");
  assert.equal(attempts, 2);
  assert.equal(sentText, "redacted");
});

test("provider host exposes bounded provider failure", async () => {
  const host = new ProviderHost({
    embedding: {
      async execute() {
        throw new Error("permanent provider failure");
      },
    },
  });
  await assert.rejects(
    () =>
      host.execute(
        {
          type: "embedding",
          workId: "W3",
          signature: "embedding-v1",
          dimensions: 3,
          items: [{ key: "u3", text: "career" }],
        },
        new AbortController().signal,
      ),
    (error: unknown) =>
      error instanceof ProviderExecutionError && error.attempts === 1,
  );
});
