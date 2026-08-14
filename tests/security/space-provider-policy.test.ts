import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { MemoriaError } from "../../src/domain/errors.js";
import { createMemoria } from "../../src/engine/create-memoria.js";
import { ProviderHost } from "../../src/providers/host.js";
import { waitFor } from "../support/remediation.js";

const localOnlyEmbedding = {
  embedding: "local-only" as const,
  reranking: "external-allowed" as const,
  enrichment: "external-allowed" as const,
};

function embeddingWork() {
  return {
    type: "embedding" as const,
    workId: "EW_local-only",
    signature: "embedding-v1",
    dimensions: 2,
    items: [{ key: "memory-1", text: "private source" }],
    spacePolicy: localOnlyEmbedding,
  };
}

test("external semantic required fails before the provider route is called", async () => {
  let calls = 0;
  const host = new ProviderHost({
    providers: {
      embedding: {
        trust: "external",
        async execute() {
          calls += 1;
          return { vectors: [{ key: "memory-1", values: [1, 0] }] };
        },
      },
    },
  });

  await assert.rejects(
    () => host.execute(embeddingWork(), new AbortController().signal),
    (error: unknown) =>
      error instanceof MemoriaError && error.code === "PROVIDER_POLICY_DENIED",
  );
  assert.equal(calls, 0);
});

test("external semantic preferred can degrade without provider egress", async () => {
  let calls = 0;
  const host = new ProviderHost({
    providers: {
      embedding: {
        trust: "external",
        async execute() {
          calls += 1;
          return { vectors: [{ key: "memory-1", values: [1, 0] }] };
        },
      },
    },
  });

  const result = await host
    .execute(embeddingWork(), new AbortController().signal)
    .catch((error: unknown) => {
      assert(
        error instanceof MemoriaError &&
          error.code === "PROVIDER_POLICY_DENIED",
      );
      return undefined;
    });
  assert.equal(result, undefined);
  assert.equal(calls, 0);
});

test("local provider route is allowed for local-only scope", async () => {
  const host = new ProviderHost({
    providers: {
      embedding: {
        trust: "local",
        async execute(work) {
          return {
            vectors: [{ key: work.items[0]!.key, values: [1, 0] }],
          };
        },
      },
    },
  });

  const result = await host.execute(
    embeddingWork(),
    new AbortController().signal,
  );
  assert.deepEqual(result, {
    type: "embeddings",
    workId: "EW_local-only",
    vectors: [{ key: "memory-1", values: [1, 0] }],
  });
});

test("native provider work carries the scoped policy to a local route", async () => {
  const dataDir = await mkdtemp(
    join(tmpdir(), "memoria-next-space-provider-policy-"),
  );
  let calls = 0;
  const memoria = await createMemoria({
    dataDir,
    providers: {
      embedding: {
        trust: "local",
        async execute(work) {
          calls += 1;
          return {
            vectors: [
              {
                key: work.items[0]!.key,
                values: [1, 0, 0],
              },
            ],
          };
        },
      },
    },
  });
  try {
    const space = await memoria.spaces.create({
      key: "local-only",
      providerPolicy: localOnlyEmbedding,
    });
    await memoria.documents.create({
      space: { id: space.id },
      documentKey: "private",
      mdx: "# Private\nlocal content",
    });
    await waitFor(() => calls > 0);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
  assert.equal(calls > 0, true);
});
