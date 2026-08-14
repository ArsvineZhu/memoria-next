import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import {
  deferredEmbeddingProvider,
  waitForPendingProvider,
  waitForSemanticBuildCoverage,
} from "../support/providers.js";

test("semantic preferred degrades until vector coverage is ready", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-semantic-query-"));
  const provider = deferredEmbeddingProvider();
  const memoria = await createMemoria({
    dataDir,
    providers: { embedding: provider },
  });
  try {
    const space = await memoria.spaces.create({ key: "personal" });
    const created = await memoria.documents.create({
      space: { id: space.id },
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    await waitForPendingProvider(provider);

    const degraded = await memoria.query({
      scope: { spaces: [space.id] },
      cue: { text: "career" },
      consistency: { preferred: ["semantic"] },
    });
    assert.equal(degraded.degraded, true);

    provider.resolveAll();
    await waitForSemanticBuildCoverage(memoria, created.authorityGeneration);

    const stillDegraded = await memoria.query({
      scope: { spaces: [space.id] },
      cue: { text: "career" },
      consistency: { preferred: ["semantic"] },
    });
    assert.equal(stillDegraded.degraded, true);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
