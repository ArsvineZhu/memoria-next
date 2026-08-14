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

test("authority mutation returns before provider completion and background pump advances readiness", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-provider-"));
  const provider = deferredEmbeddingProvider();
  const memoria = await createMemoria({
    dataDir,
    providers: { embedding: provider },
  });
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
    assert(
      BigInt(before.semanticBuildCoverage) <
        BigInt(created.authorityGeneration),
    );

    provider.resolveAll();
    await waitForSemanticBuildCoverage(memoria, created.authorityGeneration);

    const after = await memoria.status();
    assert(
      BigInt(after.semanticBuildCoverage) >=
        BigInt(created.authorityGeneration),
    );
    assert(
      BigInt(after.semanticCoverage) >= BigInt(created.authorityGeneration),
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("engine close does not wait indefinitely for provider backlog", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-provider-close-"));
  const provider = deferredEmbeddingProvider();
  const memoria = await createMemoria({
    dataDir,
    providers: { embedding: provider },
  });
  try {
    const spaceId = await memoria.createSpace("personal");
    await memoria.createMemory({
      spaceId,
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    await waitForPendingProvider(provider);

    await Promise.race([
      memoria.close(),
      new Promise<never>((_, reject) =>
        setTimeout(
          () => reject(new Error("close waited for provider backlog")),
          100,
        ),
      ),
    ]);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
