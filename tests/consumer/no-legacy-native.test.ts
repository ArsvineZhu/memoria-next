import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { test } from "node:test";

test("native package surface does not load the legacy native module", async () => {
  const loader = await readFile(resolve("native", "index.js"), "utf8");
  const packageManifest = await readFile(resolve("package.json"), "utf8");

  assert.equal(loader.includes("rust-vexus-lite"), false);
  assert.equal(loader.includes("VexusVectorStore"), false);
  assert.equal(packageManifest.includes("rust-vexus-lite"), false);
});
