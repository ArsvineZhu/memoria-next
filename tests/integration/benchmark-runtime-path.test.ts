import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { test } from "node:test";

const execFileAsync = promisify(execFile);
const repositoryRoot = join(
  dirname(fileURLToPath(import.meta.url)),
  "..",
  "..",
  "..",
);

test("retrieval benchmark calls the public Memoria runtime path", async () => {
  const source = await readFile(
    join(repositoryRoot, "benchmarks", "retrieval", "run.ts"),
    "utf8",
  );
  for (const forbidden of [
    "lexicalScore",
    "tagAssociationScore",
    "residualScore",
    "propagateTags",
  ]) {
    assert.equal(
      source.includes(forbidden),
      false,
      `benchmark runner must not contain simulator symbol ${forbidden}`,
    );
  }

  const { stdout } = await execFileAsync(
    process.execPath,
    [
      join(repositoryRoot, "node_modules", "tsx", "dist", "cli.mjs"),
      "benchmarks/retrieval/run.ts",
      "--profile",
      "tag-association",
      "--limit",
      "1",
    ],
    { cwd: repositoryRoot, maxBuffer: 8 * 1024 * 1024 },
  );
  const result = JSON.parse(stdout) as {
    runtimeBacked?: boolean;
    runtimePath?: string;
    observedChannels?: string[];
  };
  assert.equal(result.runtimeBacked, true);
  assert.equal(
    result.runtimePath,
    "createMemoria -> NativeBinding -> MemoriaRuntime::query -> QueryOperatorTrace",
  );
  assert(result.observedChannels?.includes("tag-readout"));
});
