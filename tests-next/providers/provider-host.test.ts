import assert from "node:assert/strict";
import { test } from "node:test";

import { ProviderHost } from "../../src-next/providers/host.js";

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
      items: [{ key: "u1", text: "career" }],
    },
    new AbortController().signal,
  );
  assert.equal(result.workId, "W1");
});
