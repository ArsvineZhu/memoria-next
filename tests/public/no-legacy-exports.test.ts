import assert from "node:assert/strict";
import { test } from "node:test";

import * as api from "../../src/index.js";

test("legacy 0.2 exports are absent", () => {
  assert.equal("RetrievalStrategy" in api, false);
  assert.equal("VexusVectorStore" in api, false);
  assert.equal("createMemoryEngine" in api, false);
});
