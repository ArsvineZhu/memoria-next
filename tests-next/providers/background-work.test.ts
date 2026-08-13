import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src-next/engine/create-memoria.js";
import {
  deferredEmbeddingProvider,
  waitForPendingProvider,
  waitForSemanticCoverage,
} from "../support/providers.js";

test("authority mutation returns before provider completion and background pump advances readiness", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-provider-"));
  const provider = deferredEmbeddingProvider();
  const memoria = await createMemoria({ dataDir, providers: { embedding: provider } });
  try {
    const spaceId = await memoria.createSpace("personal");
    const created = await memoria.createMemory({
      spaceId,
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    await waitForPendingProvider(provider);

    const before = await memoria.status();
    assert.equal(provider.pendingCount(), 1);
    assert(BigInt(before.semanticCoverage) < BigInt(created.authorityGeneration));

    provider.resolveAll();
    await waitForSemanticCoverage(memoria, created.authorityGeneration);

    const after = await memoria.status();
    assert(BigInt(after.semanticCoverage) >= BigInt(created.authorityGeneration));
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
