import { readFile, rm } from "node:fs/promises";
import { performance } from "node:perf_hooks";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { type QueryOptionalCapability, type SpaceId } from "../../src/index.js";
import {
  calculateQueryMetrics,
  providerCounterDelta,
  snapshotProviderCounts,
  summarizeProfileMetrics,
  type BenchmarkQueryLabel,
  type QueryMetrics,
} from "./metrics.js";
import {
  writeBenchmarkReports,
  type BenchmarkProfileReport,
} from "./report.js";
import {
  createRuntimeFixture,
  directorySize,
  readJsonLines,
  type CorpusDocument,
  type Fixture,
} from "./runtime-fixture.js";

export type Profile =
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

interface QueryCase extends BenchmarkQueryLabel {
  case: string;
  text: string;
  scope: string[];
  state?: "current" | "historical";
  entities?: string[];
  tags?: string[];
  relationKinds?: string[];
}

export interface ProfileDefinition {
  quality: "fast" | "balanced" | "thorough";
  required: QueryOptionalCapability[];
  preferred: QueryOptionalCapability[];
  expectedChannels: string[];
}

interface ProfileConfig {
  version: number;
  queryProfiles: Record<Profile, ProfileDefinition>;
  acceptance: {
    maxHardConstraintViolations: number;
    maxNdcgDrop: number;
    minRecallImprovement: number;
    maxP95LatencyIncreaseRatio: number;
    maxUnrequestedProviderCalls: number;
  };
}

const profileConfig = JSON.parse(
  await readFile(new URL("./profiles.json", import.meta.url), "utf8"),
) as ProfileConfig;

export const PROFILES = Object.keys(profileConfig.queryProfiles) as Profile[];
export const PROFILE_DEFINITIONS = profileConfig.queryProfiles;
export const PROFILE_ACCEPTANCE = profileConfig.acceptance;

const TOP_K = 10;
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
      { diagnostics: { operatorTrace: true } },
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
  let reports: BenchmarkProfileReport[];
  if (requested === "all") {
    reports = [];
    for (const profile of PROFILES)
      reports.push(await runProfile(profile, limit));
  } else if (PROFILES.includes(requested as Profile)) {
    reports = [await runProfile(requested as Profile, limit)];
  } else {
    throw new Error(
      "usage: run.ts --profile=" + [...PROFILES, "all"].join("|"),
    );
  }
  await writeBenchmarkReports(reports, {
    resultsDirectory: fileURLToPath(new URL("./results/", import.meta.url)),
  });
  console.log(
    JSON.stringify(requested === "all" ? reports : reports[0], null, 2),
  );
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

export async function runProfile(
  profile: Profile,
  limit: number,
): Promise<BenchmarkProfileReport> {
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
      const before = snapshotProviderCounts(fixture.counts);
      const response = await fixture.memoria.query(
        buildQuery(query, definition, fixture.spaceIds),
        { diagnostics: { operatorTrace: true } },
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
      queryMetrics.push(
        calculateQueryMetrics({
          query,
          returned,
          trace,
          providerDelta: providerCounterDelta(before, fixture.counts),
          latencyMs: performance.now() - queryStarted,
          fixture,
          topK: TOP_K,
        }),
      );
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
        ...summarizeProfileMetrics(
          queryMetrics,
          await directorySize(fixture.dataDir),
        ),
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
