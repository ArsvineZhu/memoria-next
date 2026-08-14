import assert from "node:assert/strict";
import test from "node:test";

import { isMemoriaError, toMemoriaError } from "../../src/domain/errors.js";

test("native corruption errors retain their stable public code", () => {
  const error = toMemoriaError(
    new Error("CORRUPTION: CAS object hash mismatch"),
  );

  assert.equal(error.code, "CORRUPTION");
  assert.equal(isMemoriaError(error, "CORRUPTION"), true);
});
