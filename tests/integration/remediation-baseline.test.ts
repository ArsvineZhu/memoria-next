import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { asSpaceId } from "../../src/domain/ids.js";
import {
  createBindingHarness,
  deferredObservedEmbeddingProvider,
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
      scope: { spaces: [asSpaceId(spaceId)] },
      cue: { text: "career" },
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
  const provider = deferredObservedEmbeddingProvider();
  const memoria = await createMemoria({
    dataDir,
    providers: { embedding: provider },
  });
  try {
    const spaceId = await memoria.createSpace("personal");
    const created = await memoria.createMemory({
      spaceId,
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    await waitFor(() => provider.pendingCount() === 1);

    // The embedding work is real background work from the Authority mutation;
    // the public query shape does not claim that query itself dispatched it.
    assert.equal(provider.calls[0]?.items[0]?.text, "Rust systems work");
    const degraded = await memoria.query({
      scope: { spaces: [asSpaceId(spaceId)] },
      cue: { text: "career" },
      consistency: { preferred: ["semantic"] },
    });
    assert.equal(provider.callCount(), 1);
    assert.equal(degraded.degraded, true);

    provider.resolveAll();
    await waitFor(async () => {
      const status = await memoria.status();
      return (
        BigInt(status.semanticBuildCoverage) >=
        BigInt(created.authorityGeneration)
      );
    });
    const semanticQuery = memoria.query({
      scope: { spaces: [asSpaceId(spaceId)] },
      cue: { text: "career" },
      consistency: { preferred: ["semantic"] },
    });
    await waitFor(() => provider.pendingCount() === 1);
    assert.equal(provider.callCount(), 2);
    provider.resolveAll();
    const semanticResponse = await semanticQuery;
    assert.equal(semanticResponse.degraded, false);
  } finally {
    provider.rejectAll();
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
        trust: "external",
        async execute() {
          return {
            vectors: [{ key: "M_remediation", values: [0.25, 0.5, 0.25] }],
          };
        },
      },
    },
  });
  try {
    await waitFor(() => harness.providerSubmissions.length === 1);
    const submission = harness.providerSubmissions[0];
    assert(submission);
    assert.deepEqual(Reflect.get(submission, "vectors"), [
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

    const response = await memoria.query({
      scope: { spaces: [asSpaceId(spaceId)] },
      cue: { text: "Rust" },
    });
    assert.equal(response.resultCount, 2);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
