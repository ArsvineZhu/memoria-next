import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { isMemoriaError } from "../../src/domain/errors.js";

test("oversized MDX fails before unbounded native work", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-limits-"));
  const memoria = await createMemoria({
    dataDir,
    resourceLimits: { maxSourceBytes: 32 },
  });
  try {
    const space = await memoria.spaces.create({ key: "personal" });
    await assert.rejects(
      () =>
        memoria.documents.create({
          space,
          mdx: "x".repeat(64),
          idempotencyKey: "oversized",
        }),
      (error: unknown) => isMemoriaError(error, "RESOURCE_LIMIT"),
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
