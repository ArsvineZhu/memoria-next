import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

const STORAGE_FILES = [
  "crates/memoria-authority/src/db.rs",
  "crates/memoria-derived/src/catalog.rs",
  "crates/memoria-adaptive/src/log.rs",
];

test("no handwritten sleep+attempt retry loops remain in Rust storage code", () => {
  for (const relativePath of STORAGE_FILES) {
    const source = readFileSync(join(process.cwd(), relativePath), "utf8");
    assert.doesNotMatch(source, /thread::sleep|RETRY_DELAYS_MS|with_retry_count/);
  }
});
