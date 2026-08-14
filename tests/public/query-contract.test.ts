import assert from "node:assert/strict";
import { test } from "node:test";

import { asSpaceId } from "../../src/domain/ids.js";
import { normalizeQuery } from "../../src/domain/query.js";

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
