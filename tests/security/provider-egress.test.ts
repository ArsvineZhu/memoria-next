import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import test from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import {
  ProviderHost,
  createProviderEgressGuard,
} from "../../src/providers/host.js";
import { waitFor } from "../support/remediation.js";
import type { EmbeddingProvider } from "../../src/providers/types.js";

test("required semantic fails before local-only provider egress", async () => {
  let calls = 0;
  const embedding: EmbeddingProvider = {
    trust: "external",
    async execute() {
      calls += 1;
      return { vectors: [{ key: "memory-1", values: [1, 0] }] };
    },
  };
  const host = new ProviderHost({
    providers: { embedding },
    onDataEgress: createProviderEgressGuard({
      embedding: false,
      rerank: false,
      enrichment: false,
    }),
  });

  await assert.rejects(
    () =>
      host.execute(
        {
          type: "embedding",
          workId: "WORK_1",
          signature: "local-only",
          dimensions: 2,
          items: [{ key: "memory-1", text: "private source" }],
        },
        new AbortController().signal,
      ),
    (error: unknown) =>
      error instanceof Error && error.message.includes("CAPABILITY_NOT_READY"),
  );
  assert.equal(calls, 0);
});

test("provider never receives literal semantic MDX tags from stored memory", async () => {
  const dataDir = await mkdtemp(
    join(tmpdir(), "memoria-next-provider-projection-"),
  );
  let received = "";
  const memoria = await createMemoria({
    dataDir,
    providers: {
      embedding: {
        trust: "external",
        async execute(work) {
          received = work.items[0]?.text ?? "";
          return {
            vectors: [
              {
                key: work.items[0]?.key ?? "memory",
                values: [1, 0, 0],
              },
            ],
          };
        },
      },
    },
  });
  try {
    const spaceId = await memoria.createSpace("personal");
    await memoria.createMemory({
      spaceId,
      documentKey: "career",
      mdx: '# Career\n<Entity ref="person:7K4Q">Ada</Entity>\n<Source id="src" ref="host-message:ABC"/>\n<Tag value="private"/>\n<State id="work">Rust systems work</State>',
    });
    await waitFor(() => received.length > 0);

    assert(received.includes("Ada"));
    assert(received.includes("Rust systems work"));
    assert(!received.includes("<Entity"));
    assert(!received.includes("person:7K4Q"));
    assert(!received.includes("<Source"));
    assert(!received.includes("host-message:ABC"));
    assert(!received.includes("<Tag"));
    assert(!received.includes('value="private"'));
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
