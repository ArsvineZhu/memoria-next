import { asSpaceId, type SpaceId } from "../../src/index.js";
import type { Fixture } from "./runtime-fixture.js";
import type { ProviderCounts } from "./fake-providers.js";

export interface BenchmarkQueryLabel {
  id: string;
  case: string;
  scope: string[];
  entities?: string[];
  relevant: Record<string, number>;
}

export interface RuntimeReturnedResult {
  id: string;
  result: {
    spaceId: string;
  };
}

export interface RuntimeTrace {
  channelsExecuted: string[];
  candidateCounts: Array<{ channel: string; count: number }>;
  activationEdgeVisits: number;
  diffusionIterations: number;
  relationExpansions: number;
  rerankRequested: boolean;
  rerankApplied: boolean;
  capabilityDegraded: boolean;
  authorityGeneration: string;
  correlationSuppressedEvidence?: number;
  tagBasisRank?: number;
}

export interface ProviderCounterDelta {
  embedding: number;
  rerank: number;
  enrichment: number;
  rerankCandidateCount: number;
}

export interface QueryMetrics {
  queryId: string;
  case: string;
  top: string[];
  recallAtK: number;
  mrr: number;
  ndcgAtK: number;
  duplicateEvidence: number;
  graphVisits: number;
  providerCalls: number;
  embeddingProviderCalls: number;
  rerankProviderCalls: number;
  enrichmentProviderCalls: number;
  lexicalSearches: number;
  semanticDirectSearches: number;
  semanticResidualSearches: number;
  annSearches: number;
  diffusionIterations: number;
  rerankCalls: number;
  rerankCandidateCount: number;
  rerankApplied: boolean;
  adaptiveApplied: boolean;
  degradedCapability: boolean;
  relationExpansions: number;
  basisRank?: number;
  latencyMs: number;
  hardConstraintViolations: number;
  trace: RuntimeTrace;
}

export interface ProfileMetrics {
  recallAt10: number;
  mrr: number;
  ndcgAt10: number;
  duplicateEvidence: number;
  duplicateEvidenceRate: number;
  hardConstraintViolationCount: number;
  p50LatencyMs: number;
  p95LatencyMs: number;
  embeddingProviderCalls: number;
  rerankProviderCalls: number;
  enrichmentProviderCalls: number;
  providerCalls: number;
  lexicalSearches: number;
  semanticDirectSearches: number;
  semanticResidualSearches: number;
  activationEdgeVisits: number;
  diffusionIterations: number;
  relationExpansions: number;
  rerankCandidateCount: number;
  rerankAppliedQueryCount: number;
  adaptiveAppliedQueryCount: number;
  degradedCapabilityCount: number;
  storeBytes: number;
}

export function snapshotProviderCounts(counts: ProviderCounts): ProviderCounts {
  return {
    embedding: counts.embedding,
    rerank: counts.rerank,
    enrichment: counts.enrichment,
    rerankCandidateCounts: [...counts.rerankCandidateCounts],
  };
}

export function providerCounterDelta(
  before: ProviderCounts,
  after: ProviderCounts,
): ProviderCounterDelta {
  return {
    embedding: after.embedding - before.embedding,
    rerank: after.rerank - before.rerank,
    enrichment: after.enrichment - before.enrichment,
    rerankCandidateCount: after.rerankCandidateCounts
      .slice(before.rerankCandidateCounts.length)
      .reduce((total, count) => total + count, 0),
  };
}

export function calculateQueryMetrics(options: {
  query: BenchmarkQueryLabel;
  returned: RuntimeReturnedResult[];
  trace: RuntimeTrace;
  providerDelta: ProviderCounterDelta;
  latencyMs: number;
  fixture: Fixture;
  topK: number;
}): QueryMetrics {
  const { query, returned, trace, providerDelta, fixture, topK } = options;
  const top = returned.slice(0, topK);
  const ranking = rankingMetrics(query, top);
  const lexicalSearches = countChannel(trace, "lexical");
  const semanticDirectSearches = countChannel(trace, "semantic-direct");
  const semanticResidualSearches = countChannel(trace, "semantic-residual");
  return {
    queryId: query.id,
    case: query.case,
    top: top.map((item) => item.id),
    ...ranking,
    duplicateEvidence: trace.correlationSuppressedEvidence ?? 0,
    graphVisits: trace.activationEdgeVisits,
    providerCalls:
      providerDelta.embedding + providerDelta.rerank + providerDelta.enrichment,
    embeddingProviderCalls: providerDelta.embedding,
    rerankProviderCalls: providerDelta.rerank,
    enrichmentProviderCalls: providerDelta.enrichment,
    lexicalSearches,
    semanticDirectSearches,
    semanticResidualSearches,
    annSearches: semanticDirectSearches + semanticResidualSearches,
    diffusionIterations: trace.diffusionIterations,
    rerankCalls: providerDelta.rerank,
    rerankCandidateCount: providerDelta.rerankCandidateCount,
    rerankApplied: trace.rerankApplied,
    adaptiveApplied: trace.channelsExecuted.includes("adaptive"),
    degradedCapability: trace.capabilityDegraded,
    relationExpansions: trace.relationExpansions,
    ...(trace.tagBasisRank === undefined
      ? {}
      : { basisRank: trace.tagBasisRank }),
    latencyMs: Number(options.latencyMs.toFixed(3)),
    hardConstraintViolations: countHardConstraintViolations(
      query,
      returned,
      fixture,
    ),
    trace,
  };
}

export function summarizeProfileMetrics(
  queryMetrics: readonly QueryMetrics[],
  storeBytes: number,
): ProfileMetrics {
  return {
    recallAt10: mean(queryMetrics.map((metric) => metric.recallAtK)),
    mrr: mean(queryMetrics.map((metric) => metric.mrr)),
    ndcgAt10: mean(queryMetrics.map((metric) => metric.ndcgAtK)),
    duplicateEvidence: sum(queryMetrics, (metric) => metric.duplicateEvidence),
    duplicateEvidenceRate: mean(
      queryMetrics.map((metric) => metric.duplicateEvidence),
    ),
    hardConstraintViolationCount: sum(
      queryMetrics,
      (metric) => metric.hardConstraintViolations,
    ),
    p50LatencyMs: percentile(
      queryMetrics.map((metric) => metric.latencyMs),
      0.5,
    ),
    p95LatencyMs: percentile(
      queryMetrics.map((metric) => metric.latencyMs),
      0.95,
    ),
    embeddingProviderCalls: sum(
      queryMetrics,
      (metric) => metric.embeddingProviderCalls,
    ),
    rerankProviderCalls: sum(
      queryMetrics,
      (metric) => metric.rerankProviderCalls,
    ),
    enrichmentProviderCalls: sum(
      queryMetrics,
      (metric) => metric.enrichmentProviderCalls,
    ),
    providerCalls: sum(queryMetrics, (metric) => metric.providerCalls),
    lexicalSearches: sum(queryMetrics, (metric) => metric.lexicalSearches),
    semanticDirectSearches: sum(
      queryMetrics,
      (metric) => metric.semanticDirectSearches,
    ),
    semanticResidualSearches: sum(
      queryMetrics,
      (metric) => metric.semanticResidualSearches,
    ),
    activationEdgeVisits: sum(queryMetrics, (metric) => metric.graphVisits),
    diffusionIterations: sum(
      queryMetrics,
      (metric) => metric.diffusionIterations,
    ),
    relationExpansions: sum(
      queryMetrics,
      (metric) => metric.relationExpansions,
    ),
    rerankCandidateCount: sum(
      queryMetrics,
      (metric) => metric.rerankCandidateCount,
    ),
    rerankAppliedQueryCount: queryMetrics.filter(
      (metric) => metric.rerankApplied,
    ).length,
    adaptiveAppliedQueryCount: queryMetrics.filter(
      (metric) => metric.adaptiveApplied,
    ).length,
    degradedCapabilityCount: queryMetrics.filter(
      (metric) => metric.degradedCapability,
    ).length,
    storeBytes,
  };
}

function rankingMetrics(
  query: BenchmarkQueryLabel,
  returned: RuntimeReturnedResult[],
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
    .slice(0, returned.length)
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
  query: BenchmarkQueryLabel,
  returned: RuntimeReturnedResult[],
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

function countChannel(trace: RuntimeTrace, channel: string): number {
  return trace.channelsExecuted.filter((entry) => entry === channel).length;
}

function sum<T>(values: readonly T[], selector: (value: T) => number): number {
  return values.reduce((total, value) => total + selector(value), 0);
}

export function percentile(
  values: readonly number[],
  quantile: number,
): number {
  const sorted = [...values].sort((left, right) => left - right);
  if (sorted.length === 0) return 0;
  const index = Math.min(
    sorted.length - 1,
    Math.ceil(sorted.length * quantile) - 1,
  );
  return Number(sorted[index].toFixed(3));
}

export function mean(values: readonly number[]): number {
  return values.length === 0
    ? 0
    : Number(
        (
          values.reduce((total, value) => total + value, 0) / values.length
        ).toFixed(4),
      );
}
