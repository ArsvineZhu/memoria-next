import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import {
  asMemoryId,
  asRevisionId,
  asSpaceId,
} from "../../src/domain/ids.js";
import { createBindingHarness } from "../support/remediation.js";

test("native request serializes the complete structured query", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-native-query-"));
  const harness = createBindingHarness();
  const memoria = await createMemoria({ dataDir, binding: harness.binding });
  const memoryId = asMemoryId("M_A3");
  const revisionId = asRevisionId("R_A3");
  const spaceId = asSpaceId("SP_A3");

  try {
    await memoria.query({
      scope: { spaces: [spaceId] },
      cue: {
        text: "career",
        tags: ["work"],
        entities: ["person:alice"],
        memories: [{ memoryId, revisionId, nodeId: "node-career" }],
      },
      constraints: {
        tags: ["must-include"],
        entities: ["project:memoria"],
        memories: [{ memoryId }],
        lifecycle: "active",
      },
      temporal: { validAt: "2026-08-01" },
      history: {
        mode: "changes",
        fromAuthorityGeneration: "2",
        toAuthorityGeneration: "7",
      },
      consistency: {
        authority: { mode: "at-least", generation: "5" },
        required: ["semantic"],
        preferred: ["reranking"],
        onNotReady: "wait",
        timeoutMs: 9000,
      },
      budget: {
        maxResults: 7,
        maxMatchesPerResult: 4,
        maxEvidenceTokens: 321,
      },
      quality: "thorough",
    });

    assert.deepEqual(harness.queryRequests, [
      {
        scope: [spaceId],
        cue: {
          text: "career",
          tags: ["work"],
          entities: ["person:alice"],
          memories: [{ memoryId, revisionId, nodeId: "node-career" }],
        },
        constraints: {
          tags: ["must-include"],
          entities: ["project:memoria"],
          memories: [{ memoryId }],
          lifecycle: "active",
        },
        temporal: { validAt: "2026-08-01" },
        history: {
          mode: "changes",
          fromAuthorityGeneration: "2",
          toAuthorityGeneration: "7",
        },
        consistency: {
          authority: { mode: "at-least", generation: "5" },
          required: ["semantic"],
          preferred: ["reranking"],
          onNotReady: "wait",
          timeoutMs: 9000,
        },
        budget: {
          maxResults: 7,
          maxMatchesPerResult: 4,
          maxEvidenceTokens: 321,
        },
        quality: "thorough",
      },
    ]);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("text-only native request does not infer semantic capability", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-native-text-query-"));
  const harness = createBindingHarness();
  const memoria = await createMemoria({ dataDir, binding: harness.binding });

  try {
    await memoria.query({
      scope: { spaces: [asSpaceId("SP_A3_TEXT")] },
      cue: { text: "career" },
    });

    assert.deepEqual(harness.queryRequests[0], {
      scope: [asSpaceId("SP_A3_TEXT")],
      cue: { text: "career" },
      history: { mode: "current" },
      consistency: {
        authority: { mode: "latest" },
        required: [],
        preferred: [],
        onNotReady: "fail",
        timeoutMs: 5000,
      },
      budget: {
        maxResults: 10,
        maxMatchesPerResult: 3,
        maxEvidenceTokens: 1500,
      },
      quality: "balanced",
    });
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
