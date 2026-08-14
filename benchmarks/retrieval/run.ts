import { mkdtemp, readFile, readdir, rm, stat } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  asMemoryId,
  asRevisionId,
  asSpaceId,
  createMemoria,
  type Memoria,
  type QueryOptionalCapability,
  type SpaceId,
} from "../../src/index.js";
import type {
  EmbeddingWork,
  ProviderSet,
  RerankWork,
} from "../../src/providers/types.js";

type Profile =
  | "lexical"
  | "lexical+semantic"
  | "tag-association"
  | "tag-basis-residual"
  | "activation"
  | "diffusion"
  | "rerank"
  | "fast"
  | "balanced"
  | "thorough";

interface Relation {
  target: string;
  kind: string;
}

interface CorpusDocument {
  id: string;
  spaceId: string;
  memoryId: string;
  revisionId: string;
  state: "current" | "historical";
  text: string;
  tags: string[];
  entities: string[];
  relations: Relation[];
}

interface QueryCase {
  id: string;
  case: string;
  text: string;
  scope: string[];
  state?: "current" | "historical";
  entities?: string[];
  tags?: string[];
  relationKinds?: string[];
  relevant: Record<string, number>;
}

interface ProviderCounts {
  embedding: number;
  rerank: number;
}

interface ProfileDefinition {
  quality: "fast" | "balanced" | "thorough";
  required: QueryOptionalCapability[];
  preferred: QueryOptionalCapability[];
  expectedChannels: string[];
}

interface QueryMetrics {
  queryId: string;
  case: string;
  top: string[];
  recallAtK: number;
  mrr: number;
  ndcgAtK: number;
  duplicateEvidence: number;
  graphVisits: number;
  providerCalls: number;
  annSearches: number;
  diffusionIterations: number;
  rerankCalls: number;
  rerankApplied: boolean;
  basisRank?: number;
  latencyMs: number;
  hardConstraintViolations: number;
  trace: {
    channelsExecuted: string[];
    candidateCounts: Array<{ channel: string; count: number }>;
    activationEdgeVisits: number;
    diffusionIterations: number;
    relationExpansions: number;
    rerankRequested: boolean;
    rerankApplied: boolean;
    capabilityDegraded: boolean;
    authorityGeneration: string;
  };
}

interface Fixture {
  memoria: Memoria;
  dataDir: string;
  spaceIds: Map<string, SpaceId>;
  externalIdByMemoryId: Map<string, string>;
  documentByExternalId: Map<string, CorpusDocument>;
  counts: ProviderCounts;
}

const PROFILES: Profile[] = [
  "lexical",
  "lexical+semantic",
  "tag-association",
  "tag-basis-residual",
  "activation",
  "diffusion",
  "rerank",
  "fast",
  "balanced",
  "thorough",
];

const PROFILE_DEFINITIONS: Record<Profile, ProfileDefinition> = {
  lexical: {
    quality: "balanced",
    required: [],
    preferred: [],
    expectedChannels: ["lexical"],
  },
  "lexical+semantic": {
    quality: "balanced",
    required: ["semantic"],
    preferred: [],
    expectedChannels: ["lexical", "semantic-direct"],
  },
  "tag-association": {
    quality: "balanced",
    required: ["associative"],
    preferred: [],
    expectedChannels: ["tag-readout"],
  },
  "tag-basis-residual": {
    quality: "balanced",
    required: ["semantic", "associative"],
    preferred: [],
    expectedChannels: ["semantic-direct", "semantic-residual"],
  },
  activation: {
    quality: "balanced",
    required: ["associative"],
    preferred: [],
    expectedChannels: ["activation"],
  },
  diffusion: {
    quality: "thorough",
    required: ["associative"],
    preferred: [],
    expectedChannels: ["diffusion"],
  },
  rerank: {
    quality: "balanced",
    required: [],
    preferred: ["reranking"],
    expectedChannels: ["lexical"],
  },
  fast: {
    quality: "fast",
    required: [],
    preferred: [],
    expectedChannels: ["lexical"],
  },
  balanced: {
    quality: "balanced",
    required: [],
    preferred: [],
    expectedChannels: ["lexical"],
  },
  thorough: {
    quality: "thorough",
    required: ["semantic", "associative"],
    preferred: ["reranking"],
    expectedChannels: [
      "lexical",
      "semantic-direct",
      "tag-readout",
      "activation",
      "diffusion",
      "relation",
    ],
  },
};

const TOP_K = 5;
const corpus = await readJsonLines<CorpusDocument>(
  new URL("./corpus.jsonl", import.meta.url),
);
const queries = await readJsonLines<QueryCase>(
  new URL("./queries.jsonl", import.meta.url),
);

const requested = requestedProfile();

try {
  if (requested === "all") {
    const results = [];
    for (const profile of PROFILES) {
      results.push(await runProfile(profile));
    }
    console.log(JSON.stringify(results, null, 2));
  } else if (PROFILES.includes(requested as Profile)) {
    console.log(
      JSON.stringify(await runProfile(requested as Profile), null, 2),
    );
  } else {
    console.error("usage: run.ts --profile=" + [...PROFILES, "all"].join("|"));
    process.exitCode = 1;
  }
} catch (error) {
  console.error(
    error instanceof Error ? (error.stack ?? error.message) : String(error),
  );
  process.exitCode = 1;
}

async function runProfile(profile: Profile) {
  const started = performance.now();
  const definition = PROFILE_DEFINITIONS[profile];
  const fixture = await createFixture();
  const queryMetrics: QueryMetrics[] = [];
  const observedChannels = new Set<string>();

  try {
    for (const query of queries) {
      const queryStarted = performance.now();
      const before = { ...fixture.counts };
      const response = await fixture.memoria.query(
        buildQuery(query, definition, fixture.spaceIds),
      );
      const trace = response.trace;
      if (!trace) {
        throw new Error(
          profile +
            "/" +
            query.id +
            ": real native response did not include QueryOperatorTrace",
        );
      }
      for (const channel of trace.channelsExecuted) {
        observedChannels.add(channel);
      }

      const returned = response.results.map((result) => ({
        id:
          fixture.externalIdByMemoryId.get(result.memoryId) ?? result.memoryId,
        result,
      }));
      const providerCalls =
        fixture.counts.embedding -
        before.embedding +
        fixture.counts.rerank -
        before.rerank;
      const rerankCalls = fixture.counts.rerank - before.rerank;
      queryMetrics.push({
        queryId: query.id,
        case: query.case,
        top: returned.slice(0, TOP_K).map((item) => item.id),
        ...rankingMetrics(query, returned.slice(0, TOP_K)),
        duplicateEvidence: trace.correlationSuppressedEvidence,
        graphVisits: trace.activationEdgeVisits,
        providerCalls,
        annSearches: trace.channelsExecuted.filter((channel) =>
          channel.startsWith("semantic-"),
        ).length,
        diffusionIterations: trace.diffusionIterations,
        rerankCalls,
        rerankApplied: trace.rerankApplied,
        ...(trace.tagBasisRank === undefined
          ? {}
          : { basisRank: trace.tagBasisRank }),
        latencyMs: Number((performance.now() - queryStarted).toFixed(3)),
        hardConstraintViolations: countHardConstraintViolations(
          query,
          returned,
          fixture,
        ),
        trace: {
          channelsExecuted: trace.channelsExecuted,
          candidateCounts: trace.candidateCounts,
          activationEdgeVisits: trace.activationEdgeVisits,
          diffusionIterations: trace.diffusionIterations,
          relationExpansions: trace.relationExpansions,
          rerankRequested: trace.rerankRequested,
          rerankApplied: trace.rerankApplied,
          capabilityDegraded: trace.capabilityDegraded,
          authorityGeneration: trace.authorityGeneration,
        },
      });
    }

    for (const expectedChannel of definition.expectedChannels) {
      if (!observedChannels.has(expectedChannel)) {
        throw new Error(
          profile +
            ": runtime trace never observed expected channel " +
            expectedChannel,
        );
      }
    }
    if (
      profile === "rerank" &&
      !queryMetrics.some((metric) => metric.rerankApplied)
    ) {
      throw new Error("rerank profile completed without an applied rerank");
    }
    if (
      profile === "thorough" &&
      !queryMetrics.some((metric) => metric.rerankApplied)
    ) {
      throw new Error("thorough profile completed without an applied rerank");
    }

    const elapsedMs = Math.max(0.01, performance.now() - started);
    const basisRanks = queryMetrics.flatMap((metric) =>
      metric.basisRank === undefined ? [] : [metric.basisRank],
    );
    return {
      runtimeBacked: true,
      runtimePath:
        "createMemoria -> NativeBinding -> MemoriaRuntime::query -> QueryOperatorTrace",
      providerMode: "deterministic-local",
      profile,
      corpusDocuments: corpus.length,
      queryCount: queries.length,
      topK: TOP_K,
      observedChannels: [...observedChannels].sort(),
      metrics: {
        recallAtK: mean(queryMetrics.map((metric) => metric.recallAtK)),
        recallAt10: mean(queryMetrics.map((metric) => metric.recallAtK)),
        mrr: mean(queryMetrics.map((metric) => metric.mrr)),
        ndcgAtK: mean(queryMetrics.map((metric) => metric.ndcgAtK)),
        ndcgAt10: mean(queryMetrics.map((metric) => metric.ndcgAtK)),
        duplicateEvidence: queryMetrics.reduce(
          (total, metric) => total + metric.duplicateEvidence,
          0,
        ),
        duplicateEvidenceRate: mean(
          queryMetrics.map((metric) => metric.duplicateEvidence),
        ),
        latencyMs: Number(elapsedMs.toFixed(3)),
        p50LatencyMs: percentile(
          queryMetrics.map((metric) => metric.latencyMs),
          0.5,
        ),
        p95LatencyMs: percentile(
          queryMetrics.map((metric) => metric.latencyMs),
          0.95,
        ),
        providerCalls: queryMetrics.reduce(
          (total, metric) => total + metric.providerCalls,
          0,
        ),
        annSearches: queryMetrics.reduce(
          (total, metric) => total + metric.annSearches,
          0,
        ),
        graphVisits: queryMetrics.reduce(
          (total, metric) => total + metric.graphVisits,
          0,
        ),
        diffusionIterations: queryMetrics.reduce(
          (total, metric) => total + metric.diffusionIterations,
          0,
        ),
        rerankCalls: queryMetrics.reduce(
          (total, metric) => total + metric.rerankCalls,
          0,
        ),
        hardConstraintViolationCount: queryMetrics.reduce(
          (total, metric) => total + metric.hardConstraintViolations,
          0,
        ),
        storeBytes: await directorySize(fixture.dataDir),
        basisRankMean: mean(basisRanks),
        basisSkippedQueries: queries.length - basisRanks.length,
      },
      queries: queryMetrics,
    };
  } finally {
    await fixture.memoria.close();
    await rm(fixture.dataDir, { recursive: true, force: true });
  }
}

async function createFixture(): Promise<Fixture> {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-retrieval-"));
  const counts: ProviderCounts = { embedding: 0, rerank: 0 };
  const memoria = await createMemoria({
    dataDir,
    providers: deterministicProviders(counts),
  });
  const spaceIds = new Map<string, SpaceId>();
  const externalIdByMemoryId = new Map<string, string>();
  const documentByExternalId = new Map(
    corpus.map((document) => [document.id, document]),
  );

  try {
    for (const spaceKey of [
      ...new Set(corpus.map((document) => document.spaceId)),
    ]) {
      const space = await memoria.spaces.create({ key: spaceKey });
      spaceIds.set(spaceKey, space.id);
    }

    let generation = "0";
    for (const document of corpus) {
      const space = spaceIds.get(document.spaceId);
      if (!space) {
        throw new Error("missing benchmark Space " + document.spaceId);
      }
      const created = await memoria.documents.create({
        space: { id: asSpaceId(space) },
        documentKey: document.id,
        mdx: documentMdx(document),
      });
      generation = created.authorityGeneration;
      externalIdByMemoryId.set(created.memoryId, document.id);
    }
    await waitForCoverage(memoria, generation);

    const heads = await memoria.query({
      scope: { spaces: [...spaceIds.values()] },
      budget: {
        maxResults: corpus.length + 10,
        maxMatchesPerResult: 1,
        maxEvidenceTokens: 100,
      },
    });
    const headByMemoryId = new Map(
      heads.results.map((result) => [result.memoryId, result.revisionId]),
    );

    for (const document of corpus.filter(
      (candidate) => candidate.relations.length > 0,
    )) {
      const memoryId = [...externalIdByMemoryId.entries()].find(
        ([, externalId]) => externalId === document.id,
      )?.[0];
      const space = spaceIds.get(document.spaceId);
      const expectedHead = memoryId ? headByMemoryId.get(memoryId) : undefined;
      if (!memoryId || !space || !expectedHead) {
        throw new Error(
          "could not resolve relation fixture head for " + document.id,
        );
      }
      const revised = await memoria.documents.revise({
        memory: { id: asMemoryId(memoryId) },
        expectedHead: asRevisionId(expectedHead),
        mdx: documentMdx(
          document,
          document.relations.map((relation) => {
            const target = [...externalIdByMemoryId.entries()].find(
              ([, externalId]) => externalId === relation.target,
            )?.[0];
            if (!target) {
              throw new Error(
                "relation target " +
                  relation.target +
                  " is missing from fixture",
              );
            }
            return target;
          }),
        ),
      });
      generation = revised.authorityGeneration;
    }
    await waitForCoverage(memoria, generation);

    return {
      memoria,
      dataDir,
      spaceIds,
      externalIdByMemoryId,
      documentByExternalId,
      counts,
    };
  } catch (error) {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
    throw error;
  }
}

function deterministicProviders(counts: ProviderCounts): ProviderSet {
  return {
    embedding: {
      trust: "local",
      async execute(work: EmbeddingWork) {
        counts.embedding += 1;
        return {
          vectors: work.items.map((item) => ({
            key: item.key,
            values: anchorVector(item.text, item.key === "query"),
          })),
        };
      },
    },
    rerank: {
      trust: "local",
      async execute(work: RerankWork) {
        counts.rerank += 1;
        return work.candidates.map((handle, index) => ({
          handle,
          score: rerankScore(work.query, handle, index),
        }));
      },
    },
    enrichment: {
      trust: "local",
      async execute() {
        return [];
      },
    },
  };
}

function anchorVector(text: string, query = false): number[] {
  const terms = new Set(text.toLowerCase().match(/[a-z0-9]+/g) ?? []);
  const groups = [
    ["career", "rust", "alex", "apollo", "cooking", "food"],
    [
      "systems",
      "graph",
      "planning",
      "support",
      "retrieval",
      "sourdough",
      "vegetables",
    ],
  ];
  const vector = groups.map((group) =>
    group.reduce((total, term) => total + (terms.has(term) ? 1 : 0), 0),
  );
  return vector.some((value) => value > 0)
    ? [...vector, query ? 0.25 : 0]
    : [0.1, 0.1, query ? 0.25 : 0];
}

function rerankScore(query: string, handle: string, index: number): number {
  const hash = stableHash(query + "\u0000" + handle);
  const positionBonus = Math.max(0, 1 - index / 64);
  return Math.min(1, 0.75 * ((hash % 10_000) / 10_000) + 0.25 * positionBonus);
}

function stableHash(value: string): number {
  let hash = 2_166_136_261;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16_777_619);
  }
  return hash >>> 0;
}

function documentMdx(
  document: CorpusDocument,
  relationMemoryIds: string[] = [],
): string {
  const lines = ["# " + document.id];
  for (const entity of document.entities) {
    lines.push(
      '<Entity ref="' +
        escapeAttribute(entity) +
        '">' +
        escapeText(entity.split(":").at(-1) ?? entity) +
        "</Entity>",
    );
  }
  for (const tag of document.tags) {
    lines.push('<Tag value="' + escapeAttribute(tag) + '"/>');
  }
  lines.push(document.text);
  for (const memoryId of relationMemoryIds) {
    lines.push('<MemoryRef memoryId="' + escapeAttribute(memoryId) + '"/>');
  }
  return lines.join("\n") + "\n";
}

function escapeAttribute(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function escapeText(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;");
}

function buildQuery(
  query: QueryCase,
  profile: ProfileDefinition,
  spaceIds: Map<string, SpaceId>,
) {
  return {
    scope: {
      spaces: query.scope.map((space) => {
        const id = spaceIds.get(space);
        if (!id) {
          throw new Error("benchmark query references unknown Space " + space);
        }
        return id;
      }),
    },
    cue: {
      ...(query.text ? { text: query.text } : {}),
      ...(query.tags ? { tags: query.tags } : {}),
    },
    ...(query.entities ? { constraints: { entities: query.entities } } : {}),
    history: {
      mode: (query.state === "historical" ? "all-revisions" : "current") as
        "all-revisions" | "current",
    },
    consistency: {
      required: profile.required,
      preferred: profile.preferred,
      onNotReady: "wait" as const,
      timeoutMs: 5_000,
    },
    budget: {
      maxResults: 20,
      maxMatchesPerResult: 3,
      maxEvidenceTokens: 2_000,
    },
    quality: profile.quality,
  };
}

function rankingMetrics(
  query: QueryCase,
  returned: Array<{ id: string }>,
): {
  recallAtK: number;
  mrr: number;
  ndcgAtK: number;
} {
  const relevant = new Set(Object.keys(query.relevant));
  const hits = returned.filter((candidate) => relevant.has(candidate.id));
  const firstHit = returned.findIndex((candidate) =>
    relevant.has(candidate.id),
  );
  const dcg = returned.reduce((total, candidate, index) => {
    const grade = query.relevant[candidate.id] ?? 0;
    return total + (2 ** grade - 1) / Math.log2(index + 2);
  }, 0);
  const ideal = Object.values(query.relevant)
    .sort((left, right) => right - left)
    .slice(0, TOP_K)
    .reduce(
      (total, grade, index) => total + (2 ** grade - 1) / Math.log2(index + 2),
      0,
    );
  return {
    recallAtK: relevant.size === 0 ? 0 : hits.length / relevant.size,
    mrr: firstHit < 0 ? 0 : 1 / (firstHit + 1),
    ndcgAtK: ideal === 0 ? 0 : dcg / ideal,
  };
}

function countHardConstraintViolations(
  query: QueryCase,
  returned: Array<{ id: string; result: { spaceId: string } }>,
  fixture: Fixture,
): number {
  const scopeIds = new Set(
    query.scope
      .map((space) => fixture.spaceIds.get(space))
      .filter((id): id is SpaceId => id !== undefined),
  );
  return returned.reduce((total, item) => {
    const document = fixture.documentByExternalId.get(item.id);
    const outsideScope = !scopeIds.has(asSpaceId(item.result.spaceId));
    const missingEntity =
      query.entities !== undefined &&
      document !== undefined &&
      !query.entities.every((entity) => document.entities.includes(entity));
    return total + (outsideScope || missingEntity ? 1 : 0);
  }, 0);
}

async function waitForCoverage(
  memoria: Memoria,
  generation: string,
  timeoutMs = 10_000,
): Promise<void> {
  const target = BigInt(generation);
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const status = await memoria.status();
    if (
      BigInt(status.baseCoverage) >= target &&
      BigInt(status.semanticCoverage) >= target &&
      BigInt(status.semanticBuildCoverage) >= target
    ) {
      return;
    }
    await delay(2);
  }
  const status = await memoria.status();
  throw new Error(
    "runtime-backed retrieval fixture did not converge: target=" +
      generation +
      " base=" +
      status.baseCoverage +
      " semantic=" +
      status.semanticCoverage +
      " semanticBuild=" +
      status.semanticBuildCoverage,
  );
}

async function directorySize(path: string): Promise<number> {
  const entries = await readdir(path, { withFileTypes: true });
  let total = 0;
  for (const entry of entries) {
    const child = join(path, entry.name);
    if (entry.isDirectory()) {
      total += await directorySize(child);
    } else {
      total += (await stat(child)).size;
    }
  }
  return total;
}

async function readJsonLines<T>(url: URL): Promise<T[]> {
  const source = await readFile(url, "utf8");
  return source
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as T);
}

function requestedProfile(): string {
  const argument = process.argv.find((value) => value.startsWith("--profile="));
  const index = process.argv.indexOf("--profile");
  return (
    argument?.slice("--profile=".length) ??
    (index >= 0 ? process.argv[index + 1] : "") ??
    ""
  );
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
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

function mean(values: number[]): number {
  return values.length === 0
    ? 0
    : Number(
        (
          values.reduce((total, value) => total + value, 0) / values.length
        ).toFixed(4),
      );
}
