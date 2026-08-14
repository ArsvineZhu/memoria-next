import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";

test("logical purge makes the target inaccessible immediately", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-purge-"));
  const memoria = await createMemoria({ dataDir });
  try {
    const space = await memoria.spaces.create({ key: "personal" });
    const created = await memoria.documents.create({
      space,
      documentKey: "private",
      mdx: "# Private\nPurge me",
    });
    const plan = await memoria.admin.planPurge({
      memory: { id: created.memoryId },
    });
    const completed = await memoria.admin.executePurge({ planId: plan.id });

    assert.equal(completed.state, "completed");
    const response = await memoria.query({
      scope: { spaces: [space.id] },
      cue: { text: "Purge" },
    });
    assert.equal(response.results.length, 0);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
