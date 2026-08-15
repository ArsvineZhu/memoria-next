import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";

type InventoryEntry = {
  class: "DOMAIN" | "ALGORITHM" | "INTEGRATION" | "INFRASTRUCTURE" | "REVIEW_REQUIRED";
};

function runInventory(): Record<string, InventoryEntry> {
  const result = spawnSync(
    process.execPath,
    ["scripts/library-first/inventory.mjs", "--json"],
    { cwd: process.cwd(), encoding: "utf8" },
  );

  assert.equal(result.status, 0, result.stderr || "inventory command failed");
  return JSON.parse(result.stdout) as Record<string, InventoryEntry>;
}

test("inventory classifies known infrastructure modules", () => {
  const inventory = runInventory();

  assert.equal(
    inventory["crates/memoria-derived/src/catalog.rs"].class,
    "INFRASTRUCTURE",
  );
  assert.equal(
    inventory["crates/memoria-query/src/algorithms/tag_basis.rs"].class,
    "ALGORITHM",
  );
  assert.equal(inventory["crates/memoria-mdx/src/ir.rs"].class, "DOMAIN");

  const reviewRequired = Object.entries(inventory)
    .filter(([, entry]) => entry.class === "REVIEW_REQUIRED")
    .map(([path]) => path);
  assert.deepEqual(reviewRequired, []);
});
