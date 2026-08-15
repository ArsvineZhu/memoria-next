import assert from "node:assert/strict";
import { test } from "node:test";

import * as api from "../../src/index.js";
import { normalizeQuery } from "../../src/domain/query.js";
import { asSpaceId } from "../../src/domain/ids.js";

test("public domain surface remains stable during infrastructure substitution", () => {
  for (const name of [
    "createMemoria",
    "Memoria",
    "MemoriaError",
    "serializeRestrictedMdx",
    "createAgentTools",
  ]) {
    assert.equal(name in api, true, `missing public export: ${name}`);
  }
});

test("query cue and hard constraints remain distinct", () => {
  const query = normalizeQuery({
    scope: { spaces: [asSpaceId("SP_contract")] },
    cue: { text: "career", entities: ["person:alice"] },
    constraints: { entities: ["person:bob"], lifecycle: "active" },
  });

  assert.deepEqual(query.cue?.entities, ["person:alice"]);
  assert.deepEqual(query.constraints?.entities, ["person:bob"]);
  assert.equal(query.constraints?.lifecycle, "active");
});
