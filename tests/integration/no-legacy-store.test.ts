import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { isMemoriaError } from "../../src/domain/errors.js";

test("legacy Store layout is rejected instead of initialized as Next", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-legacy-store-"));
  try {
    await writeFile(join(dataDir, "memory.sqlite"), "legacy");

    await assert.rejects(
      () => createMemoria({ dataDir }),
      (error: unknown) =>
        isMemoriaError(error, "UNSUPPORTED_STORE_FORMAT"),
    );
  } finally {
    await rm(dataDir, { recursive: true, force: true });
  }
});
