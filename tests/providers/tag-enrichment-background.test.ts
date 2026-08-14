import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import {
  observedTagEnrichmentProvider,
  waitForTagEnrichment,
} from "../support/providers.js";

test("background enrichment receives a versioned projection without Raw MDX", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-tag-enrichment-"));
  const enrichment = observedTagEnrichmentProvider(["career", "systems"]);
  const memoria = await createMemoria({
    dataDir,
    providers: {
      embedding: {
        trust: "external",
        async execute(work) {
          return {
            vectors: [
              {
                key: work.items[0]?.key ?? "query",
                values: Array.from({ length: work.dimensions }, () => 0),
              },
            ],
          };
        },
      },
      enrichment,
    },
  });
  try {
    const spaceId = await memoria.createSpace("personal");
    await memoria.createMemory({
      spaceId,
      documentKey: "career",
      mdx: '# Career\n<Tag value="private"/>\nRust systems work',
    });
    await waitForTagEnrichment(enrichment);

    const [work] = enrichment.works;
    assert.equal(work.projection.version, 1);
    assert.equal(work.projection.maxTags, 8);
    assert(!work.projection.content.includes("<Tag"));
    assert(!work.projection.content.includes('value="private"'));
    assert(work.projection.content.includes("Rust systems work"));
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
