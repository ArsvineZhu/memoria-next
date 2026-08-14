import { rm } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  asSpaceId,
  type QueryOptionalCapability,
  type SpaceId,
} from "../../src/index.js";
import {
  createRuntimeFixture,
  directorySize,
  readJsonLines,
  type CorpusDocument,
  type Fixture,
} from "./runtime-fixture.js";

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

export const PROFILES: Profile[] = [
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

export const PROFILE_DEFINITIONS: Record<Profile, ProfileDefinition> = {
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

export async function runSingleBenchmarkQuery(options: {
  fixture: string;
}): Promise<{
  source: "runtime";
  queryId: string;
  trace: NonNullable<Awaited<ReturnType<Fixture["memoria"]["query"]>>["trace"]>;
  resultIds: string[];
}> {
  const query = resolveFixtureQuery(options.fixture);
  const profile = PROFILE_DEFINITIONS[profileForQuery(query)];
  const fixture = await createRuntimeFixture(corpus);
  try {
    const response = await fixture.memoria.query(
      buildQuery(query, profile, fixture.spaceIds),
    );
    if (!response.trace) {
      throw new Error(
        "real native response did not include QueryOperatorTrace",
      );
    }
    return {
      source: "runtime",
      queryId: query.id,
      trace: response.trace,
      resultIds: response.results.map((result) => result.memoryId),
    };
  } finally {
    await fixture.memoria.close();
    await rm(fixture.dataDir, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  const requested = requestedProfile();
  const limit = requestedLimit();
  if (requested === "all") {
    const results = [];
    for (const profile of PROFILES) {
      results.push(await runProfile(profile, limit));
    }
    console.log(JSON.stringify(results, null, 2));
  } else if (PROFILES.includes(requested as Profile)) {
    console.log(
      JSON.stringify(await runProfile(requested as Profile, limit), null, 2),
    );
  } else {
    throw new Error(
      "usage: run.ts --profile=" + [...PROFILES, "all"].join("|"),
    );
  }
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  try {
    await main();
  } catch (error) {
    console.error(
      error instanceof Error ? (error.stack ?? error.message) : String(error),
    );
    process.exitCode = 1;
  }
}

async function runProfile(profile: Profile, limit: number) {
  const started = performance.now();
  const definition = PROFILE_DEFINITIONS[profile];
  const fixture = await createRuntimeFixture(corpus);
  const queryMetrics: QueryMetrics[] = [];
  const observedChannels = new Set<string>();
  const profileQueries = queries.filter((query) =>
    queryMatchesProfile(query, profile),
  );
  const selectedQueries = (
    profileQueries.length > 0 ? profileQueries : queries
  ).slice(0, limit);

  try {
    for (const query of selectedQueries) {
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
        before.rerank +
        fixture.counts.enrichment -
        before.enrichment;
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
      (profile === "rerank" || profile === "thorough") &&
      !queryMetrics.some((metric) => metric.rerankApplied)
    ) {
      throw new Error(profile + " profile completed without an applied rerank");
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
      queryCount: selectedQueries.length,
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
        basisSkippedQueries: selectedQueries.length - basisRanks.length,
      },
      queries: queryMetrics,
    };
  } finally {
    await fixture.memoria.close();
    await rm(fixture.dataDir, { recursive: true, force: true });
  }
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

function resolveFixtureQuery(fixture: string): QueryCase {
  const aliases: Record<string, string> = {
    "tag-association-01": "q-tag-rust",
    "semantic-residual-01": "q-current-career",
  };
  const queryId = aliases[fixture] ?? fixture;
  const query = queries.find((candidate) => candidate.id === queryId);
  if (!query) {
    throw new Error("unknown benchmark fixture " + fixture);
  }
  return query;
}

function profileForQuery(query: QueryCase): Profile {
  if (query.id === "q-tag-rust") return "tag-association";
  if (query.id === "q-current-career") return "balanced";
  return "lexical";
}

function queryMatchesProfile(query: QueryCase, profile: Profile): boolean {
  switch (profile) {
    case "tag-association":
      return query.id === "q-tag-rust";
    case "tag-basis-residual":
      return query.id === "q-current-career";
    case "activation":
    case "diffusion":
      return query.id === "q-relation-support";
    default:
      return false;
  }
}

function rankingMetrics(
  query: QueryCase,
  returned: Array<{ id: string }>,
): { recallAtK: number; mrr: number; ndcgAtK: number } {
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

function requestedProfile(): string {
  const argument = process.argv.find((value) => value.startsWith("--profile="));
  const index = process.argv.indexOf("--profile");
  return (
    argument?.slice("--profile=".length) ??
    (index >= 0 ? process.argv[index + 1] : "") ??
    ""
  );
}

function requestedLimit(): number {
  const argument = process.argv.find((value) => value.startsWith("--limit="));
  const index = process.argv.indexOf("--limit");
  const raw =
    argument?.slice("--limit=".length) ??
    (index >= 0 ? process.argv[index + 1] : undefined);
  if (raw === undefined) return queries.length;
  const limit = Number(raw);
  if (!Number.isSafeInteger(limit) || limit < 1) {
    throw new Error("--limit must be a positive integer");
  }
  return Math.min(limit, queries.length);
}

function percentile(values: number[], quantile: number): number {
  const sorted = [...values].sort((left, right) => left - right);
  if (sorted.length === 0) return 0;
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
