import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src-next/engine/create-memoria.js";

test("engine opens reports status and closes", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-engine-"));
  try {
    const memoria = await createMemoria({ dataDir });
    const status = await memoria.status();
    assert.equal(status.authorityGeneration, "0");
    await memoria.close();
    await assert.rejects(() => memoria.status());
  } finally {
    await rm(dataDir, { recursive: true, force: true });
  }
});
