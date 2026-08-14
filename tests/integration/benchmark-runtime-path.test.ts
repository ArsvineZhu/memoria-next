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
          operator: string;
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
    .map(
      (line) =>
        JSON.parse(line) as {
          id: string;
          scenario: string;
          text: string;
          scope: string[];
          tags?: string[];
          entities?: string[];
          state?: "current" | "historical";
          profile: string;
        },
    );
  const preservationCase = preservationCases.find(
    (item) => item.scenario === "runtime-serving-path",
  );
  assert(preservationCase);
  const preservationQuery = preservationQueries.find(
    (item) => item.scenario === preservationCase.scenario,
  );
  assert(preservationQuery);
  assert.equal(preservationQuery.profile, preservationCase.operator);

  const evalScript =
    "import { runSingleBenchmarkQuery } from './benchmarks/retrieval/run.ts'; " +
    `const query = ${JSON.stringify(preservationQuery)}; ` +
    "const result = await runSingleBenchmarkQuery({ fixture: query.id, query }); " +
    "console.log(JSON.stringify(result));";
  const { stdout } = await execFileAsync(
    process.execPath,
    ["--import", "tsx", "--input-type=module", "--eval", evalScript],
    { cwd: repositoryRoot, maxBuffer: 8 * 1024 * 1024 },
  );
  const result = JSON.parse(stdout) as {
    source?: string;
    queryId?: string;
    trace?: { channelsExecuted: string[] };
  };
  assert.equal(result.source, "runtime");
  assert.equal(result.queryId, preservationQuery.id);
  assert(result.trace);
  assert(result.trace.channelsExecuted.length > 0);
  for (const expectedChannel of preservationCase.expectedChannels) {
    assert(result.trace.channelsExecuted.includes(expectedChannel));
  }
});
