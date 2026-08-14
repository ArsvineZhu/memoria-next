import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { loadNativeBinding } from "../../src/native/binding.js";

test("required semantic wait does not return a degraded lexical response", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-readiness-"));
  const binding = loadNativeBinding();
  const store = binding.openStore(dataDir);

  try {
    const spaceId = binding.authorityCreateSpace(store, "readiness");
    binding.authorityMutate(store, {
      spaceId,
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    const step = await binding.queryStart(store, {
      scope: [spaceId],
      cue: { text: "career" },
      history: { mode: "current" },
      consistency: {
        authority: { mode: "latest" },
        required: ["semantic"],
        preferred: [],
        onNotReady: "wait",
        timeoutMs: 250,
      },
      budget: {
        maxResults: 10,
        maxMatchesPerResult: 3,
        maxEvidenceTokens: 1500,
      },
      quality: "balanced",
    });

    assert("state" in step);
    if (step.state === "complete") {
      assert.equal(
        step.response?.degraded,
        false,
        "required semantic wait must not complete with a degraded fallback",
      );
    } else if (step.work.type === "query-embedding") {
      const resumed = await binding.queryResume(store, step.operationId, {
        type: "embeddings",
        workId: step.work.workId,
        vectors: [{ key: "query", values: [1, 0, 0] }],
      });
      if ("state" in resumed && resumed.state === "complete") {
        assert.equal(
          resumed.response?.degraded,
          false,
          "required semantic wait must not complete with a degraded fallback",
        );
      }
    }
  } finally {
    binding.closeStore(store);
    await rm(dataDir, { recursive: true, force: true });
  }
});
