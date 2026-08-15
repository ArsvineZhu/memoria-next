import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

type FixtureFile = [path: string, content: string];

function runPolicy(files: FixtureFile[]) {
  const root = mkdtempSync(join(tmpdir(), "memoria-library-policy-"));
  try {
    for (const [path, content] of files) {
      const target = join(root, path);
      mkdirSync(join(target, ".."), { recursive: true });
      writeFileSync(target, content);
    }
    const result = spawnSync(
      process.execPath,
      [
        "scripts/library-first/verify-policy.mjs",
        "--root",
        root,
        "--all-files",
      ],
      { cwd: process.cwd(), encoding: "utf8" },
    );
    return `${result.stdout}\n${result.stderr}`;
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("policy rejects new generic infrastructure module without allowlist reason", () => {
  const output = runPolicy([
    ["src/infrastructure/cache-layer.rs", "pub struct CacheLayer;\n"],
  ]);

  assert.match(output, /generic infrastructure/i);
});

test("policy rejects new custom retry loop marker", () => {
  const output = runPolicy([
    [
      "src/providers/adapter.ts",
      "let attempts = 0; while (attempts < 3) { await sleep(10); attempts += 1; }\n",
    ],
  ]);

  assert.match(output, /handwritten retry/i);
});

test("policy rejects reintroducing direct Tantivy or USearch after LanceDB migration marker", () => {
  const output = runPolicy([
    ["docs/reports/lancedb-poc.md", "decision: PASS\n"],
    ["crates/memoria-derived/Cargo.toml", "[dependencies]\ntantivy = \"0.1\"\n"],
  ]);

  assert.match(output, /banned retrieval dependency/i);
});

test("policy accepts an explicitly justified domain exception", () => {
  const output = runPolicy([
    ["src/infrastructure/derived-manifest.rs", "pub struct DerivedManifest;\n"],
    [
      "scripts/library-first/domain-exceptions.json",
      JSON.stringify({ "DerivedManifest": "snapshot semantics remain Memoria-owned" }),
    ],
  ]);

  assert.doesNotMatch(output, /violation/i);
});
