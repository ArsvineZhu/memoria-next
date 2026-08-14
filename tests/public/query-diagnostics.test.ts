import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";

test("operator trace is opt-in and contains no source payload", async () => {
  const dataDir = await mkdtemp(
    join(tmpdir(), "memoria-next-query-diagnostics-"),
  );
  const memoria = await createMemoria({ dataDir });

  try {
    const space = await memoria.spaces.create({ key: "diagnostics" });
    await memoria.documents.create({
      space: { id: space.id },
      documentKey: "private-cue",
      mdx: "# Private source marker\ninternal-source-marker-7f3a",
    });

    const query = {
      scope: { spaces: [space.id] },
      cue: { text: "private source marker" },
    };
    const normal = await memoria.query(query);
    assert.equal(normal.trace, undefined);

    const diagnostic = await memoria.query(query, {
      diagnostics: { operatorTrace: true },
    });
    assert(diagnostic.trace);
    assert(diagnostic.trace.channelsExecuted.length > 0);
    const serialized = JSON.stringify(diagnostic);
    assert(!serialized.includes("internal-source-marker-7f3a"));
    assert(!serialized.includes("Private source marker"));
    assert(!serialized.includes("vector"));
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
