import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import type { MemoriaQuery } from "../../src/engine/memoria.js";
import {
  createBindingHarness,
  deferredObservedEmbeddingProvider,
  removeAuthoritySourceObjects,
  waitFor,
} from "../support/remediation.js";

test("text-only query performs zero provider calls", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-remediation-a1-"));
  const provider = deferredObservedEmbeddingProvider();
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
    await waitFor(() => provider.pendingCount() === 1);

    const callsBeforeQuery = provider.callCount();
    const response = await memoria.query({
      scope: [spaceId],
      text: "career",
    });

    assert.equal(provider.callCount(), callsBeforeQuery);
    assert.equal(response.degraded, false);
  } finally {
    provider.rejectAll();
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("semantic preferred is explicit and may request provider work", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-remediation-a1-"));
  const harness = createBindingHarness();
  const memoria = await createMemoria({
    dataDir,
    binding: harness.binding,
  });
  try {
    await memoria.query({
      scope: ["SP_remediation"],
      text: "career",
    });
    await memoria.query({
      scope: ["SP_remediation"],
      text: "career",
      semanticPreference: "preferred",
    } as unknown as MemoriaQuery);

    assert.equal(harness.queryRequests.length, 2);
    assert.deepEqual(harness.queryRequests[1], {
      scope: ["SP_remediation"],
      text: "career",
      semanticPreference: "preferred",
    });
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("provider embedding result preserves vector values across native resume", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-remediation-a1-"));
  const harness = createBindingHarness({
    providerWork: [
      {
        workId: "EW_remediation",
        workType: "embedding",
        signature: "embedding-v1",
        dimensions: 3,
        items: [{ key: "M_remediation", text: "career" }],
        candidates: [],
      },
    ],
  });
  const memoria = await createMemoria({
    dataDir,
    binding: harness.binding,
    providers: {
      embedding: {
        async execute() {
          return {
            vectors: [[0.25, 0.5, 0.25]],
          };
        },
      },
    },
  });
  try {
    await waitFor(() => harness.providerSubmissions.length === 1);
    assert.deepEqual(harness.providerSubmissions[0]?.embeddings, [
      { key: "M_remediation", values: [0.25, 0.5, 0.25] },
    ]);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("normal current query does not reparse every Authority source", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-remediation-a1-"));
  const memoria = await createMemoria({ dataDir });
  try {
    const spaceId = await memoria.createSpace("personal");
    await memoria.createMemory({
      spaceId,
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    await memoria.createMemory({
      spaceId,
      documentKey: "graph",
      mdx: "# Graph\nRust retrieval notes",
    });

    const beforeDeletion = await memoria.query({
      scope: [spaceId],
      text: "Rust",
    });
    assert.equal(beforeDeletion.resultCount, 2);

    assert.equal(await removeAuthoritySourceObjects(dataDir), 2);

    await assert.doesNotReject(async () => {
      const afterDeletion = await memoria.query({
        scope: [spaceId],
        text: "Rust",
      });
      assert.equal(afterDeletion.resultCount, 2);
    });
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
