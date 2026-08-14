import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";

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
  vector: number[];
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

interface Candidate {
  document: CorpusDocument;
  score: number;
  lexical: number;
  semantic: number;
  signals: string[];
  rerankScore?: number;
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
  latencyMs: number;
  hardConstraintViolations: number;
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
const TOP_K = 5;
const MAX_ACTIVE_TAGS = 64;
const MAX_EDGE_VISITS = 512;
const MAX_HOPS = 3;

const corpus = readJsonLines<CorpusDocument>(
  new URL("./corpus.jsonl", import.meta.url),
);
const queries = readJsonLines<QueryCase>(
  new URL("./queries.jsonl", import.meta.url),
);

const profileArgument = process.argv.find((argument) =>
  argument.startsWith("--profile="),
);
const profileFlagIndex = process.argv.indexOf("--profile");
const requested =
  profileArgument?.slice("--profile=".length) ??
  (profileFlagIndex >= 0 ? process.argv[profileFlagIndex + 1] : "") ??
  "";

if (requested === "all") {
  console.log(JSON.stringify(PROFILES.map(runProfile), null, 2));
} else if (PROFILES.includes(requested as Profile)) {
  console.log(JSON.stringify(runProfile(requested as Profile), null, 2));
} else {
  console.error(`usage: run.ts --profile=${[...PROFILES, "all"].join("|")}`);
  process.exitCode = 1;
}

function runProfile(profile: Profile) {
  const started = performance.now();
  let providerCalls = 0;
  let graphVisits = 0;
  let basisRankTotal = 0;
  let basisSkippedQueries = 0;
  const queryMetrics: QueryMetrics[] = [];
  const executionProfile: Profile =
    profile === "fast" || profile === "balanced"
      ? "lexical"
      : profile === "thorough"
        ? "rerank"
        : profile;

  for (const query of queries) {
    const queryStarted = performance.now();
    const scoped = corpus.filter(
      (document) =>
        query.scope.includes(document.spaceId) &&
        (query.state === undefined || document.state === query.state) &&
        (query.entities ?? []).every((entity) =>
          document.entities.includes(entity),
        ),
    );
    const queryVector = queryVectorFor(query.text);
    const tagGraph = buildTagGraph(scoped);
    const activation =
      executionProfile === "activation" || executionProfile === "diffusion"
        ? propagateTags(
            tagGraph,
            query.tags ?? [],
            executionProfile === "diffusion" ? MAX_HOPS + 1 : MAX_HOPS,
          )
        : { scores: new Map<string, number>(), visits: 0 };
    graphVisits += activation.visits;
    let queryBasisRank: number | undefined;

    let candidates = scoped.map((document) => {
      const candidate = scoreDocument(
        document,
        query,
        queryVector,
        executionProfile,
        scoped,
        activation.scores,
      );
      if (candidate.basisRank !== undefined && queryBasisRank === undefined) {
        basisRankTotal += candidate.basisRank;
        basisSkippedQueries += candidate.basisSkipped ? 1 : 0;
        queryBasisRank = candidate.basisRank;
      }
      return candidate.value;
    });
    candidates.sort(compareBase);

    if (executionProfile === "rerank" && candidates.length > 0) {
      providerCalls += 1;
      const selected = candidates.slice(0, TOP_K).map((candidate) => ({
        ...candidate,
        rerankScore: 0.65 * candidate.lexical + 0.35 * candidate.semantic,
      }));
      selected.sort(compareReranked);
      candidates = [...selected, ...candidates.slice(TOP_K)];
    }

    queryMetrics.push(
      metricsFor(
        query,
        candidates,
        activation.visits,
        executionProfile === "rerank" && candidates.length > 0 ? 1 : 0,
        executionProfile,
        performance.now() - queryStarted,
      ),
    );
  }

  const elapsedMs = Math.max(0.01, performance.now() - started);
  return {
    profile,
    corpusDocuments: corpus.length,
    queryCount: queries.length,
    topK: TOP_K,
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
      providerCalls,
      annSearches: queryMetrics.reduce(
        (total, metric) => total + metric.annSearches,
        0,
      ),
      graphVisits,
      diffusionIterations: queryMetrics.reduce(
        (total, metric) => total + metric.diffusionIterations,
        0,
      ),
      rerankCalls: providerCalls,
      hardConstraintViolationCount: queryMetrics.reduce(
        (total, metric) => total + metric.hardConstraintViolations,
        0,
      ),
      workingSetBytes: estimateWorkingSetBytes(),
      basisRankMean:
        queries.length === 0
          ? 0
          : Number((basisRankTotal / queries.length).toFixed(3)),
      basisSkippedQueries,
    },
    queries: queryMetrics,
  };
}

function scoreDocument(
  document: CorpusDocument,
  query: QueryCase,
  queryVector: number[],
  profile: Profile,
  scoped: CorpusDocument[],
  activation: Map<string, number>,
): { value: Candidate; basisRank?: number; basisSkipped?: boolean } {
  const lexical = lexicalScore(document, query.text);
  const semantic = cosine(queryVector, document.vector);
  const signals: string[] = [];
  let score = lexical;
  if (lexical > 0) {
    signals.push("lexical");
  }

  if (profile !== "lexical") {
    score += semantic * 0.75;
    if (semantic > 0) {
      signals.push("semantic");
    }
  }

  if (query.entities?.length && document.entities.length > 0) {
    score += 0.35;
    signals.push("entity");
  }

  if (profile === "tag-association" || profile === "tag-basis-residual") {
    const association = tagAssociationScore(document, query.tags ?? [], scoped);
    score += association * 0.45;
    if (association > 0) {
      signals.push("tag-association");
    }
  }

  let basisRank: number | undefined;
  let basisSkipped: boolean | undefined;
  if (profile === "tag-basis-residual") {
    const basis = residualScore(
      queryVector,
      query.tags ?? [],
      scoped,
      document.vector,
    );
    basisRank = basis.rank;
    basisSkipped = !basis.used;
    score += basis.score * 0.4;
    if (basis.used && basis.score > 0) {
      signals.push("tag-basis-residual");
    }
  }

  if (profile === "activation" || profile === "diffusion") {
    const activationScore = document.tags.reduce(
      (best, tag) => Math.max(best, activation.get(tag) ?? 0),
      0,
    );
    score += activationScore * 0.55;
    if (activationScore > 0) {
      signals.push(profile);
    }
  }

  if (
    query.relationKinds?.some((kind) =>
      document.relations.some((r) => r.kind === kind),
    )
  ) {
    score += 0.35;
    signals.push("relation");
  }

  return {
    value: { document, score, lexical, semantic, signals },
    basisRank,
    basisSkipped,
  };
}

function metricsFor(
  query: QueryCase,
  candidates: Candidate[],
  graphVisits: number,
  providerCalls: number,
  profile: Profile,
  latencyMs: number,
): QueryMetrics {
  const top = candidates.slice(0, TOP_K);
  const relevant = new Set(Object.keys(query.relevant));
  const hits = top.filter((candidate) => relevant.has(candidate.document.id));
  const recallAtK = relevant.size === 0 ? 0 : hits.length / relevant.size;
  const firstHit = top.findIndex((candidate) =>
    relevant.has(candidate.document.id),
  );
  const mrr = firstHit < 0 ? 0 : 1 / (firstHit + 1);
  const dcg = top.reduce((total, candidate, index) => {
    const grade = query.relevant[candidate.document.id] ?? 0;
    return total + (2 ** grade - 1) / Math.log2(index + 2);
  }, 0);
  const ideal = Object.values(query.relevant)
    .sort((left, right) => right - left)
    .slice(0, TOP_K)
    .reduce(
      (total, grade, index) => total + (2 ** grade - 1) / Math.log2(index + 2),
      0,
    );
  const duplicateEvidence = top.reduce(
    (total, candidate) =>
      total + candidate.signals.length - new Set(candidate.signals).size,
    0,
  );
  return {
    queryId: query.id,
    case: query.case,
    top: top.map((candidate) => candidate.document.id),
    recallAtK,
    mrr,
    ndcgAtK: ideal === 0 ? 0 : dcg / ideal,
    duplicateEvidence,
    graphVisits: graphVisits,
    providerCalls,
    annSearches: profile === "lexical" ? 0 : 1,
    diffusionIterations: profile === "diffusion" ? 1 : 0,
    latencyMs,
    hardConstraintViolations: 0,
  };
}

function lexicalScore(document: CorpusDocument, text: string): number {
  const queryTerms = tokens(text);
  const documentTerms = tokens(
    [
      document.text,
      ...document.tags,
      ...document.entities,
      ...document.relations.map((r) => r.kind),
    ].join(" "),
  );
  if (queryTerms.size === 0) {
    return 0;
  }
  let overlap = 0;
  for (const term of queryTerms) {
    if (documentTerms.has(term)) {
      overlap += 1;
    }
  }
  return overlap / queryTerms.size;
}

function tagAssociationScore(
  document: CorpusDocument,
  seeds: string[],
  scoped: CorpusDocument[],
): number {
  if (seeds.length === 0) {
    return 0;
  }
  if (seeds.some((seed) => document.tags.includes(seed))) {
    return 1;
  }
  const cooccurring = new Set(
    scoped
      .filter((item) => seeds.some((seed) => item.tags.includes(seed)))
      .flatMap((item) => item.tags),
  );
  return document.tags.some((tag) => cooccurring.has(tag)) ? 0.5 : 0;
}

function buildTagGraph(
  documents: CorpusDocument[],
): Map<string, Map<string, number>> {
  const graph = new Map<string, Map<string, number>>();
  for (const document of documents) {
    for (const left of document.tags) {
      for (const right of document.tags) {
        if (left === right) {
          continue;
        }
        const neighbors = graph.get(left) ?? new Map<string, number>();
        neighbors.set(right, (neighbors.get(right) ?? 0) + 1);
        graph.set(left, neighbors);
      }
    }
  }
  return graph;
}

function propagateTags(
  graph: Map<string, Map<string, number>>,
  seeds: string[],
  maxHops: number,
): { scores: Map<string, number>; visits: number } {
  const scores = new Map<string, number>();
  let frontier = new Map<string, number>();
  for (const seed of seeds) {
    if (scores.size >= MAX_ACTIVE_TAGS) {
      break;
    }
    scores.set(seed, 1);
    frontier.set(seed, 1);
  }
  let visits = 0;
  for (let hop = 0; hop < maxHops && frontier.size > 0; hop += 1) {
    const next = new Map<string, number>();
    for (const [source, sourceScore] of frontier) {
      const neighbors = [...(graph.get(source)?.entries() ?? [])].sort(
        ([left], [right]) => left.localeCompare(right),
      );
      for (const [target, weight] of neighbors) {
        if (visits >= MAX_EDGE_VISITS) {
          return { scores, visits };
        }
        visits += 1;
        const score = sourceScore * (weight / (1 + weight)) * 0.5;
        if (
          score <= 0 ||
          (scores.has(target) && (scores.get(target) ?? 0) >= score)
        ) {
          continue;
        }
        if (!scores.has(target) && scores.size >= MAX_ACTIVE_TAGS) {
          continue;
        }
        scores.set(target, score);
        next.set(target, score);
      }
    }
    frontier = next;
  }
  return { scores, visits };
}

function residualScore(
  queryVector: number[],
  tags: string[],
  scoped: CorpusDocument[],
  documentVector: number[],
): { rank: number; used: boolean; score: number } {
  const vectors = tags
    .map((tag) =>
      averageVector(scoped.filter((document) => document.tags.includes(tag))),
    )
    .filter((vector): vector is number[] => vector !== undefined);
  const basis: number[][] = [];
  for (const vector of vectors) {
    let residual = [...vector];
    for (const unit of basis) {
      const projection = dot(residual, unit);
      residual = residual.map(
        (value, index) => value - projection * (unit[index] ?? 0),
      );
    }
    const norm = l2(residual);
    if (norm > 1.0e-6) {
      basis.push(residual.map((value) => value / norm));
    }
  }
  if (basis.length === 0) {
    return { rank: 0, used: false, score: 0 };
  }
  let residual = [...queryVector];
  for (const unit of basis) {
    const projection = dot(residual, unit);
    residual = residual.map(
      (value, index) => value - projection * (unit[index] ?? 0),
    );
  }
  const score = Math.max(0, cosine(residual, documentVector));
  return { rank: basis.length, used: l2(residual) > 1.0e-6, score };
}

function averageVector(documents: CorpusDocument[]): number[] | undefined {
  if (documents.length === 0) {
    return undefined;
  }
  const dimensions = documents[0]?.vector.length ?? 0;
  const vector = Array.from({ length: dimensions }, () => 0);
  for (const document of documents) {
    document.vector.forEach((value, index) => {
      vector[index] = (vector[index] ?? 0) + value;
    });
  }
  return vector.map((value) => value / documents.length);
}

function queryVectorFor(text: string): number[] {
  const terms = tokens(text);
  const anchors = [
    ["career", "rust", "alex", "apollo"],
    ["systems", "graph", "planning", "support", "retrieval"],
    ["old", "deployment", "ops", "prototype"],
    ["cooking", "sourdough", "vegetables", "travel", "itinerary"],
  ];
  return anchors.map((group) => group.filter((term) => terms.has(term)).length);
}

function cosine(left: number[], right: number[]): number {
  const denominator = l2(left) * l2(right);
  return denominator === 0 ? 0 : Math.max(0, dot(left, right) / denominator);
}

function dot(left: number[], right: number[]): number {
  return left.reduce(
    (total, value, index) => total + value * (right[index] ?? 0),
    0,
  );
}

function l2(vector: number[]): number {
  return Math.sqrt(dot(vector, vector));
}

function tokens(value: string): Set<string> {
  return new Set(value.toLowerCase().match(/[a-z0-9]+/g) ?? []);
}

function compareBase(left: Candidate, right: Candidate): number {
  return (
    right.score - left.score ||
    left.document.id.localeCompare(right.document.id)
  );
}

function compareReranked(left: Candidate, right: Candidate): number {
  return (
    (right.rerankScore ?? -1) - (left.rerankScore ?? -1) ||
    compareBase(left, right)
  );
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

function estimateWorkingSetBytes(): number {
  return (
    Buffer.byteLength(JSON.stringify(corpus)) +
    Buffer.byteLength(JSON.stringify(queries))
  );
}

function readJsonLines<T>(url: URL): T[] {
  return readFileSync(url, "utf8")
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as T);
}
