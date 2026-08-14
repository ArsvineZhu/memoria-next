import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  createPortableExport,
  importPortablePackage,
} from "../../src/admin/governance.js";
import type {
  NativePortableImportRequest,
  NativePortableImportResult,
} from "../../src/native/protocol.js";

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

test("native import receives the complete package for source-aware remapping", async () => {
  const root = await mkdtemp(
    join(tmpdir(), "memoria-next-import-native-test-"),
  );
  const packagePath = join(root, "package.json");
  const requests: NativePortableImportRequest[] = [];
  const result: NativePortableImportResult = {
    targetSpaceId: "SP_TARGET",
    mappings: [
      { sourceId: "M_SOURCE", targetId: "M_TARGET" },
      { sourceId: "M_OTHER", targetId: "M_OTHER_TARGET" },
    ],
    unresolvedExternalReferences: ["M_EXTERNAL"],
  };
  await writeFile(
    packagePath,
    JSON.stringify({
      format: "memoria-portable-v1",
      originStoreId: "ST_SOURCE",
      scope: ["SP_SOURCE"],
      memories: [
        {
          sourceId: "M_SOURCE",
          spaceId: "SP_SOURCE",
          revisionId: "R_SOURCE",
          mdx: '<MemoryRef memoryId="M_OTHER"/>',
        },
        {
          sourceId: "M_OTHER",
          spaceId: "SP_SOURCE",
          revisionId: "R_OTHER",
          mdx: '<MemoryRef ref="M_EXTERNAL"/>',
        },
      ],
    }),
    "utf8",
  );
  const engine = {
    async exportMemories() {
      return [];
    },
    async createSpace() {
      return "SP_FALLBACK";
    },
    async createMemory() {
      return { memoryId: "M_FALLBACK", authorityGeneration: "1" };
    },
    async importPortable(request: NativePortableImportRequest) {
      requests.push(request);
      return result;
    },
  };

  try {
    const imported = await importPortablePackage(engine, {
      packagePath,
      targetSpace: { key: "target" },
      idempotencyKey: "native-import-1",
    });
    assert.deepEqual(imported.mappings.memories, result.mappings);
    assert.deepEqual(imported.unresolvedExternalReferences, ["M_EXTERNAL"]);
    assert.equal(requests.length, 1);
    assert.equal(requests[0]?.memories.length, 2);
    assert.equal(
      requests[0]?.memories[0]?.mdx,
      '<MemoryRef memoryId="M_OTHER"/>',
    );
    assert.match(requests[0]?.requestFingerprint ?? "", /^[0-9a-f]{64}$/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("native import retries are delegated to durable idempotency", async () => {
  const root = await mkdtemp(join(tmpdir(), "memoria-next-import-retry-test-"));
  const packagePath = join(root, "package.json");
  await writeFile(
    packagePath,
    JSON.stringify({
      format: "memoria-portable-v1",
      originStoreId: "ST_SOURCE",
      scope: ["SP_SOURCE"],
      memories: [
        {
          sourceId: "M_SOURCE",
          spaceId: "SP_SOURCE",
          revisionId: "R_SOURCE",
          mdx: "# Stable",
        },
      ],
    }),
    "utf8",
  );
  const durable = new Map<string, NativePortableImportResult>();
  let calls = 0;
  const engine = {
    async exportMemories() {
      return [];
    },
    async createSpace() {
      return "SP_FALLBACK";
    },
    async createMemory() {
      return { memoryId: "M_FALLBACK", authorityGeneration: "1" };
    },
    async importPortable(request: NativePortableImportRequest) {
      calls += 1;
      const existing = durable.get(request.idempotencyKey);
      if (existing) {
        return existing;
      }
      const created: NativePortableImportResult = {
        targetSpaceId: "SP_DURABLE",
        mappings: [{ sourceId: "M_SOURCE", targetId: "M_LOCAL" }],
        unresolvedExternalReferences: [],
      };
      durable.set(request.idempotencyKey, created);
      return created;
    },
  };

  try {
    const input = {
      packagePath,
      targetSpace: { key: "target" },
      idempotencyKey: "retry-import",
    };
    const first = await importPortablePackage(engine, input);
    const second = await importPortablePackage(engine, input);
    assert.deepEqual(second, first);
    assert.equal(calls, 2);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
