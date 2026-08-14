import assert from "node:assert/strict";
import { test } from "node:test";

import {
  providerHostWithTagEnrichment,
  tagEnrichmentWork,
} from "../support/providers.js";

test("tag enrichment provider returns bounded candidates for the requested work", async () => {
  const host = providerHostWithTagEnrichment(["career", "systems"]);
  const result = await host.execute(
    tagEnrichmentWork("W-tag-1"),
    new AbortController().signal,
  );
  assert.equal(result.workId, "W-tag-1");
  assert.equal(result.type, "enrichment");
  if (result.type !== "enrichment") {
    throw new Error("expected an enrichment result");
  }
  assert.deepEqual(result.tags, ["career", "systems"]);
  assert(
    result.tags?.every(
      (value) => typeof value === "string" && value.length > 0,
    ),
  );
});
