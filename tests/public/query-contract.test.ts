import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { MemoriaError, toMemoriaError } from "../../src/domain/errors.js";
import { asMemoryId, asRevisionId, asSpaceId } from "../../src/domain/ids.js";
import { normalizeQuery } from "../../src/domain/query.js";
import { createBindingHarness } from "../support/remediation.js";

test("scope.spaces is required and non-empty", () => {
  assert.throws(
    () => normalizeQuery({ scope: { spaces: [] } }),
    /scope\.spaces must contain at least one SpaceId/,
  );
});

test("text cue alone has no semantic capability", () => {
  const normalized = normalizeQuery({
    scope: { spaces: [asSpaceId("SP_personal")] },
    cue: { text: "career" },
  });

  assert.deepEqual(normalized.scope, { spaces: ["SP_personal"] });
  assert.deepEqual(normalized.cue, { text: "career" });
  assert.deepEqual(normalized.consistency.required, []);
  assert.deepEqual(normalized.consistency.preferred, []);
});

test("semantic required/preferred remain explicit after normalization", () => {
  const normalized = normalizeQuery({
    scope: { spaces: [asSpaceId("SP_personal")] },
    cue: { text: "career" },
    consistency: {
      required: ["semantic"],
      preferred: ["reranking", "adaptive"],
    },
  });

  assert.deepEqual(normalized.consistency.required, ["semantic"]);
  assert.deepEqual(normalized.consistency.preferred, ["reranking", "adaptive"]);
});

test("cue entities and constraint entities remain separate", () => {
  const normalized = normalizeQuery({
    scope: { spaces: [asSpaceId("SP_personal")] },
    cue: { entities: ["person:alice"] },
    constraints: { entities: ["person:bob"] },
  });

  assert.deepEqual(normalized.cue, { entities: ["person:alice"] });
  assert.deepEqual(normalized.constraints, { entities: ["person:bob"] });
});

test("nested query intent remains separate and explicit after normalization", () => {
  const cueMemory = {
    memoryId: asMemoryId("M_cue"),
    revisionId: asRevisionId("R_cue"),
    nodeId: "node-cue",
  };
  const constraintMemory = {
    memoryId: asMemoryId("M_constraint"),
    nodeId: "node-constraint",
  };
  const normalized = normalizeQuery({
    scope: {
      spaces: [asSpaceId("SP_personal"), { id: asSpaceId("SP_work") }],
    },
    cue: {
      text: "career",
      tags: ["work"],
      entities: ["person:alice"],
      memories: [cueMemory],
    },
    constraints: {
      tags: ["private"],
      entities: ["person:bob"],
      memories: [constraintMemory],
      lifecycle: "retired",
    },
    temporal: { validAt: "2026-08-01T00:00:00Z" },
    history: {
      mode: "changes",
      fromAuthorityGeneration: "7",
      toAuthorityGeneration: "9",
    },
    consistency: {
      authority: { mode: "exact", generation: "9" },
      required: ["semantic"],
      preferred: ["adaptive"],
      onNotReady: "wait",
      timeoutMs: 250,
    },
    budget: {
      maxResults: 5,
      maxMatchesPerResult: 2,
      maxEvidenceTokens: 900,
    },
    quality: "thorough",
  });

  assert.deepEqual(normalized, {
    scope: { spaces: ["SP_personal", "SP_work"] },
    cue: {
      text: "career",
      tags: ["work"],
      entities: ["person:alice"],
      memories: [cueMemory],
    },
    constraints: {
      tags: ["private"],
      entities: ["person:bob"],
      memories: [constraintMemory],
      lifecycle: "retired",
    },
    temporal: { validAt: "2026-08-01T00:00:00Z" },
    history: {
      mode: "changes",
      fromAuthorityGeneration: "7",
      toAuthorityGeneration: "9",
    },
    consistency: {
      authority: { mode: "exact", generation: "9" },
      required: ["semantic"],
      preferred: ["adaptive"],
      onNotReady: "wait",
      timeoutMs: 250,
    },
    budget: {
      maxResults: 5,
      maxMatchesPerResult: 2,
      maxEvidenceTokens: 900,
    },
    quality: "thorough",
  });
});

test("all authority consistency modes remain explicit after normalization", () => {
  const modes = [
    { mode: "latest" },
    { mode: "at-least", generation: "8" },
    { mode: "exact", generation: "9" },
  ] as const;

  for (const authority of modes) {
    const normalized = normalizeQuery({
      scope: { spaces: [asSpaceId("SP_personal")] },
      consistency: { authority },
    });
    assert.deepEqual(normalized.consistency.authority, authority);
  }
});

test("default budget is 10/3/1500 and quality balanced", () => {
  const normalized = normalizeQuery({
    scope: { spaces: [{ id: asSpaceId("SP_personal") }] },
  });

  assert.deepEqual(normalized.history, { mode: "current" });
  assert.deepEqual(normalized.consistency, {
    authority: { mode: "latest" },
    required: [],
    preferred: [],
    onNotReady: "fail",
    timeoutMs: 5000,
  });
  assert.deepEqual(normalized.budget, {
    maxResults: 10,
    maxMatchesPerResult: 3,
    maxEvidenceTokens: 1500,
  });
  assert.equal(normalized.quality, "balanced");
  assert.deepEqual(normalized.scope, { spaces: ["SP_personal"] });
});

test("capabilities cannot be duplicated or outside the locked set", () => {
  assert.throws(
    () =>
      normalizeQuery({
        scope: { spaces: [asSpaceId("SP_personal")] },
        consistency: {
          required: ["semantic"],
          preferred: ["semantic"],
        },
      }),
    /duplicate.*capability/i,
  );

  assert.throws(
    () =>
      normalizeQuery({
        scope: { spaces: [asSpaceId("SP_personal")] },
        consistency: { required: ["unsupported" as never] },
      }),
    /capability.*unsupported/i,
  );
});

test("query limits reject negative or zero values", () => {
  const scope = { spaces: [asSpaceId("SP_personal")] };

  assert.throws(
    () => normalizeQuery({ scope, consistency: { timeoutMs: -1 } }),
    /timeoutMs must be non-negative/,
  );
  assert.throws(
    () => normalizeQuery({ scope, budget: { maxResults: 0 } }),
    /maxResults must be positive/,
  );
  assert.throws(
    () => normalizeQuery({ scope, budget: { maxMatchesPerResult: 0 } }),
    /maxMatchesPerResult must be positive/,
  );
  assert.throws(
    () => normalizeQuery({ scope, budget: { maxEvidenceTokens: 0 } }),
    /maxEvidenceTokens must be positive/,
  );
});

test("toMemoriaError preserves structured query validation errors", () => {
  const message = "QUERY_ERROR: scope.spaces must contain at least one SpaceId";
  const mapped = toMemoriaError(new Error(message));

  assert(mapped instanceof MemoriaError);
  assert.equal(mapped.code, "QUERY_ERROR");
  assert.equal(mapped.message, message);
});

test("Memoria.query maps query validation to public MemoriaError", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-query-error-"));
  const memoria = await createMemoria({
    dataDir,
    binding: createBindingHarness().binding,
  });
  try {
    await assert.rejects(
      () => memoria.query({ scope: { spaces: [] } }),
      (error: unknown) =>
        error instanceof MemoriaError &&
        error.code === "QUERY_ERROR" &&
        error.message ===
          "QUERY_ERROR: scope.spaces must contain at least one SpaceId",
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("openReadSession maps query validation to public MemoriaError", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-session-error-"));
  const memoria = await createMemoria({
    dataDir,
    binding: createBindingHarness().binding,
  });
  try {
    await assert.rejects(
      () => memoria.openReadSession({ scope: { spaces: [] } }),
      (error: unknown) =>
        error instanceof MemoriaError &&
        error.code === "QUERY_ERROR" &&
        error.message ===
          "QUERY_ERROR: scope.spaces must contain at least one SpaceId",
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
