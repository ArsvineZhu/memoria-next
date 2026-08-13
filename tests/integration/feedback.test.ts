import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { asRevisionId } from "../../src/domain/ids.js";

test("feedback pins the revision returned by retrieval", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-feedback-"));
  const memoria = await createMemoria({ dataDir });
  try {
    const space = await memoria.spaces.create({ key: "personal" });
    const created = await memoria.documents.create({
      space: { id: space.id },
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    const query = await memoria.query({
      scope: [space.id],
      text: "career",
    });
    const original = query.results.find(
      (result) => result.memoryId === created.memoryId,
    );
    assert.ok(original);

    const revised = await memoria.documents.revise({
      memory: { id: created.memoryId },
      expectedHead: asRevisionId(original.revisionId),
      mdx: "# Career\nUpdated Rust systems work",
    });
    assert.notEqual(revised.revisionId, original.revisionId);

    const committed = await memoria.feedback.submit({
      retrievalId: query.retrievalId,
      idempotencyKey: "fb-1",
      events: [{ resultId: original.resultId, outcome: "used" }],
    });

    assert.equal(committed.events[0]?.revisionId, original.revisionId);
    assert.notEqual(committed.events[0]?.revisionId, revised.revisionId);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
