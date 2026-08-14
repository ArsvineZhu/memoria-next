import assert from "node:assert/strict";
import test from "node:test";

import {
  discoverScopedEntities,
  type EntityObservation,
} from "../../src/admin/governance.js";

test("entity discovery does not expose unauthorized observations", () => {
  const observations: EntityObservation[] = [
    { entityRef: "person:alex", surface: "Alex", spaceId: "public" },
    { entityRef: "person:alex-secret", surface: "Alex", spaceId: "secret" },
  ];

  const result = discoverScopedEntities(observations, {
    surface: "Alex",
    allowedSpaces: ["public"],
  });

  assert.deepEqual(result, [observations[0]]);
  assert.equal(JSON.stringify(result).includes("secret"), false);
});
