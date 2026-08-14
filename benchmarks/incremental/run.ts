import { execFile } from "node:child_process";
import { mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { asMemoryId, asRevisionId, createMemoria } from "../../src/index.js";

interface IncrementalCase {
  id: string;
  description: string;
  next?: string;
  operation?: "move-space";
  expected: {
    embeddingCalls: number;
    contentEmbeddingReused?: boolean;
    vectorPayloadReused?: boolean;
  };
}

interface Fixture {
  version: number;
  baseline: string;
  cases: IncrementalCase[];
}

interface ProviderCounts {
  embedding: number;
  enrichment: number;
  rerank: number;
}

interface CaseResult {
  id: string;
  description: string;
  operation: string;
  startupMs: number;
  writeMs: number;
  queryMs: number;
  storeBytes: number;
  rssBytes: number;
  embeddingCalls: number;
  enrichmentCalls: number;
  rerankCalls: number;
  semanticCoverage: string;
  authorityGeneration: string;
  vectorPayloadReused?: boolean;
}

const fixture = JSON.parse(
  await readFile(new URL("./cases.json", import.meta.url), "utf8"),
) as Fixture;
if (fixture.version !== 1 || fixture.cases.length === 0) {
  throw new Error("incremental fixture version or cases are invalid");
}

const results: CaseResult[] = [];
for (const editCase of fixture.cases) {
  if (editCase.operation === "move-space") {
    results.push(await runSpaceMoveCase(editCase));
  } else {
    results.push(await runRevisionCase(fixture.baseline, editCase));
  }
}

const revisionResults = results.filter(
  (result) => result.operation === "revise",
);
const failures = results.flatMap((result) => {
  const editCase = fixture.cases.find(
    (candidate) => candidate.id === result.id,
  );
  if (!editCase) {
    return [`${result.id}: fixture case is missing`];
  }
  const errors: string[] = [];
  if (result.embeddingCalls !== editCase.expected.embeddingCalls) {
    errors.push(
      `${result.id}: expected ${editCase.expected.embeddingCalls} embedding call(s), got ${result.embeddingCalls}`,
    );
  }
  if (
    editCase.expected.vectorPayloadReused !== undefined &&
    result.vectorPayloadReused !== editCase.expected.vectorPayloadReused
  ) {
    errors.push(
      `${result.id}: expected vectorPayloadReused=${editCase.expected.vectorPayloadReused}, got ${result.vectorPayloadReused}`,
    );
  }
  return errors;
});

const summary = {
  platform: `${process.platform}/${process.arch}`,
  node: process.version,
  fixtureVersion: fixture.version,
  caseCount: results.length,
  providerCalls: {
    embedding: results.reduce((sum, result) => sum + result.embeddingCalls, 0),
    enrichment: results.reduce(
      (sum, result) => sum + result.enrichmentCalls,
      0,
    ),
    rerank: results.reduce((sum, result) => sum + result.rerankCalls, 0),
  },
  latencyMs: {
    startupP50: percentile(
      revisionResults.map((result) => result.startupMs),
      0.5,
    ),
    startupP95: percentile(
      revisionResults.map((result) => result.startupMs),
      0.95,
    ),
    writeP50: percentile(
      revisionResults.map((result) => result.writeMs),
      0.5,
    ),
    writeP95: percentile(
      revisionResults.map((result) => result.writeMs),
      0.95,
    ),
    queryP50: percentile(
      revisionResults.map((result) => result.queryMs),
      0.5,
    ),
    queryP95: percentile(
      revisionResults.map((result) => result.queryMs),
      0.95,
    ),
  },
  storeBytes: {
    p50: percentile(
      revisionResults.map((result) => result.storeBytes),
      0.5,
    ),
    p95: percentile(
      revisionResults.map((result) => result.storeBytes),
      0.95,
    ),
    max: Math.max(...revisionResults.map((result) => result.storeBytes)),
  },
  peakRssBytes: Math.max(...results.map((result) => result.rssBytes)),
  results,
};

console.log(JSON.stringify(summary, null, 2));
if (failures.length > 0) {
  console.error(`incremental gate failed:\n- ${failures.join("\n- ")}`);
  process.exitCode = 1;
} else {
  console.log("incremental invalidation/provider-cost gate passed");
}

async function runRevisionCase(
  baseline: string,
  editCase: IncrementalCase,
): Promise<CaseResult> {
  if (editCase.next === undefined) {
    throw new Error(`${editCase.id}: revision case is missing next source`);
  }
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-incremental-"));
  const counts: ProviderCounts = { embedding: 0, enrichment: 0, rerank: 0 };
  const startupStarted = performance.now();
  const memoria = await createMemoria({
    dataDir,
    providers: {
      embedding: {
        async execute(work) {
          counts.embedding += 1;
          return {
            vectors: work.items.map((item) => ({
              key: item.key,
              values: Array.from({ length: work.dimensions }, (_, index) =>
                index === 0 ? 1 : 0,
              ),
            })),
          };
        },
      },
      enrichment: {
        async execute() {
          counts.enrichment += 1;
          return ["derived"];
        },
      },
    },
  });
  const startupMs = performance.now() - startupStarted;
  try {
    const space = await memoria.spaces.create({ key: `space-${editCase.id}` });
    const created = await memoria.documents.create({
      space,
      mdx: baseline,
    });
    await waitForEmbeddingCoverage(memoria, created.authorityGeneration);
    await waitForProviderCalls(counts, 1, 1);

    const before = await memoria.query({
      scope: { spaces: [space.id] },
      cue: { text: "Rust" },
    });
    const current = before.results[0];
    if (!current) {
      throw new Error(`${editCase.id}: baseline query returned no result`);
    }

    const enrichmentBeforeRevision = counts.enrichment;
    const writeStarted = performance.now();
    const revised = await memoria.documents.revise({
      memory: { id: asMemoryId(current.memoryId) },
      expectedHead: asRevisionId(current.revisionId),
      mdx: editCase.next,
    });
    const writeMs = performance.now() - writeStarted;
    const expectedEnrichmentCalls = enrichmentBeforeRevision + 1;
    await waitForProviderCalls(counts, expectedEnrichmentCalls, 0);
    if (editCase.expected.embeddingCalls > 0) {
      await waitForEmbeddingCoverage(memoria, revised.authorityGeneration);
    }

    const queryStarted = performance.now();
    await memoria.query({
      scope: { spaces: [space.id] },
      cue: { text: "Rust" },
    });
    const queryMs = performance.now() - queryStarted;
    const status = await memoria.status();
    const storeBytes = await directorySize(dataDir);
    return {
      id: editCase.id,
      description: editCase.description,
      operation: "revise",
      startupMs,
      writeMs,
      queryMs,
      storeBytes,
      rssBytes: process.memoryUsage().rss,
      embeddingCalls: counts.embedding - 1,
      enrichmentCalls: counts.enrichment,
      rerankCalls: counts.rerank,
      semanticCoverage: status.semanticCoverage,
      authorityGeneration: status.authorityGeneration,
    };
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
}

async function runSpaceMoveCase(
  editCase: IncrementalCase,
): Promise<CaseResult> {
  const root = fileURLToPath(new URL("../..", import.meta.url));
  const cargo = process.platform === "win32" ? "cargo.exe" : "cargo";
  const started = performance.now();
  await promisify(execFile)(
    cargo,
    [
      "test",
      "-p",
      "memoria-derived",
      "moving_membership_reuses_immutable_payload_and_filters_by_space",
      "--quiet",
    ],
    { cwd: root },
  );
  const elapsed = performance.now() - started;
  return {
    id: editCase.id,
    description: editCase.description,
    operation: "move-space",
    startupMs: elapsed,
    writeMs: elapsed,
    queryMs: 0,
    storeBytes: 0,
    rssBytes: process.memoryUsage().rss,
    embeddingCalls: 0,
    enrichmentCalls: 0,
    rerankCalls: 0,
    semanticCoverage: "not-applicable",
    authorityGeneration: "not-applicable",
    vectorPayloadReused: true,
  };
}

async function waitForEmbeddingCoverage(
  memoria: { status(): Promise<{ semanticCoverage: string }> },
  generation: string,
): Promise<void> {
  await waitFor(
    async () =>
      BigInt((await memoria.status()).semanticCoverage) >= BigInt(generation),
  );
}

async function waitForProviderCalls(
  counts: ProviderCounts,
  enrichmentCalls: number,
  embeddingCalls: number,
): Promise<void> {
  await waitFor(
    () =>
      counts.enrichment >= enrichmentCalls &&
      counts.embedding >= embeddingCalls,
  );
}

async function waitFor(check: () => boolean | Promise<boolean>): Promise<void> {
  const deadline = Date.now() + 5_000;
  while (!(await check()) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 2));
  }
  if (!(await check())) {
    throw new Error(
      "incremental provider work did not converge before timeout",
    );
  }
}

function percentile(values: number[], quantile: number): number {
  const sorted = [...values].sort((left, right) => left - right);
  if (sorted.length === 0) {
    return 0;
  }
  const index = Math.min(
    sorted.length - 1,
    Math.ceil(sorted.length * quantile) - 1,
  );
  return Number(sorted[index].toFixed(3));
}

async function directorySize(directory: string): Promise<number> {
  const entries = await readdir(directory, { withFileTypes: true });
  let total = 0;
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      total += await directorySize(path);
    } else if (entry.isFile()) {
      total += (await stat(path)).size;
    }
  }
  return total;
}
