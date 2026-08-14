import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { serializeRestrictedMdx } from "../../src/authoring/serialize.js";
import { asRevisionId } from "../../src/domain/ids.js";
import { isMemoriaError } from "../../src/domain/errors.js";

test("revise requires explicit expected head", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-domain-"));
  const memoria = await createMemoria({ dataDir });
  try {
    const space = await memoria.spaces.create({ key: "personal" });
    const created = await memoria.documents.create({
      space: { id: space.id },
      documentKey: "career",
      mdx: "# Career\n",
      idempotencyKey: "create-career",
    });
    const retried = await memoria.documents.create({
      space: { id: space.id },
      documentKey: "career",
      mdx: "# Career\n",
      idempotencyKey: "create-career",
    });
    assert.equal(retried.memoryId, created.memoryId);

    await assert.rejects(
      () =>
        memoria.documents.revise({
          memory: { id: created.memoryId },
          expectedHead: asRevisionId(`R_${"00".repeat(32)}`),
          mdx: "# Career\nchanged",
        }),
      (error: unknown) => isMemoriaError(error, "HEAD_CONFLICT"),
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("restricted authoring serializer emits declarative MDX", () => {
  assert.equal(
    serializeRestrictedMdx({
      title: "Career",
      tags: ["work"],
      body: "Rust systems",
    }),
    '# Career\n<Tag value="work"/>\nRust systems\n',
  );
});

test("query accepts structured scope and cue intent", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-query-api-"));
  const memoria = await createMemoria({ dataDir });
  try {
    const space = await memoria.spaces.create({ key: "personal" });
    const created = await memoria.documents.create({
      space: { id: space.id },
      documentKey: "career",
      mdx: "# Career\nRust systems",
    });

    const response = await memoria.query({
      scope: { spaces: [{ id: space.id }] },
      cue: { text: "career" },
    });

    assert.equal(
      response.results.some((result) => result.memoryId === created.memoryId),
      true,
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
