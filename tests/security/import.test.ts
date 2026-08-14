import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  createPortableExport,
  importPortablePackage,
} from "../../src/admin/governance.js";

test("import creates new local ids and resolves package-internal references", async () => {
  const root = await mkdtemp(join(tmpdir(), "memoria-next-import-test-"));
  let nextId = 0;
  await writeFile(join(root, "STORE"), "ST_SOURCE");
  const engine = {
    async exportMemories() {
      return [
        {
          sourceId: "M_SOURCE",
          spaceId: "SP_SOURCE",
          revisionId: "R_SOURCE",
          mdx: "# Career",
        },
      ];
    },
    async createSpace() {
      return "SP_TARGET";
    },
    async createMemory() {
      nextId += 1;
      return { memoryId: "M_TARGET_" + nextId, authorityGeneration: "1" };
    },
  };

  try {
    const exported = await createPortableExport(root, engine, {
      scope: ["SP_SOURCE"],
    });
    const result = await importPortablePackage(engine, {
      packagePath: exported.path,
      targetSpace: { key: "imported" },
      idempotencyKey: "import-1",
    });

    assert.notEqual(
      result.mappings.memories[0]?.sourceId,
      result.mappings.memories[0]?.targetId,
    );
    assert.deepEqual(result.unresolvedExternalReferences, []);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
