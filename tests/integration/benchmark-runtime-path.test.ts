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

  const preservationCases = (
    await readFile(
      join(
        repositoryRoot,
        "benchmarks",
        "retrieval",
        "preservation-corpus.jsonl",
      ),
      "utf8",
    )
  )
    .split(/\r?\n/)
    .filter(Boolean)
    .map(
      (line) =>
        JSON.parse(line) as {
          scenario: string;
          queryId: string;
          expectedChannels: string[];
        },
    );
  const preservationQueries = (
    await readFile(
      join(
        repositoryRoot,
        "benchmarks",
        "retrieval",
        "preservation-queries.jsonl",
      ),
      "utf8",
    )
  )
    .split(/\r?\n/)
    .filter(Boolean)
    .map((line) => JSON.parse(line) as { id: string; scenario: string });
  const preservationCase = preservationCases.find(
    (item) => item.scenario === "runtime-serving-path",
  );
  assert(preservationCase);
  assert(
    preservationQueries.some(
      (item) => item.scenario === preservationCase.scenario,
    ),
  );

  const evalScript =
    "import { runSingleBenchmarkQuery } from './benchmarks/retrieval/run.ts'; " +
    "const result = await runSingleBenchmarkQuery({ fixture: 'tag-association-01' }); " +
    "console.log(JSON.stringify(result));";
  const { stdout } = await execFileAsync(
    process.execPath,
    ["--import", "tsx", "--input-type=module", "--eval", evalScript],
    { cwd: repositoryRoot, maxBuffer: 8 * 1024 * 1024 },
  );
  const result = JSON.parse(stdout) as {
    source?: string;
    trace?: { channelsExecuted: string[] };
  };
  assert.equal(result.source, "runtime");
  assert(result.trace);
  assert(result.trace.channelsExecuted.length > 0);
  for (const expectedChannel of preservationCase.expectedChannels) {
    assert(result.trace.channelsExecuted.includes(expectedChannel));
  }
});
