import type { QueryOptionalCapability } from "../../src/index.js";
import type { ProfileMetrics } from "./metrics.js";

export interface AcceptanceInput {
  baseline: Pick<
    ProfileMetrics,
    | "recallAt10"
    | "ndcgAt10"
    | "p95LatencyMs"
    | "embeddingProviderCalls"
    | "rerankProviderCalls"
    | "enrichmentProviderCalls"
  >;
  candidate: Pick<
    ProfileMetrics,
    | "hardConstraintViolationCount"
    | "recallAt10"
    | "ndcgAt10"
    | "p95LatencyMs"
    | "embeddingProviderCalls"
    | "rerankProviderCalls"
    | "enrichmentProviderCalls"
  >;
  targetRecallImprovement?: number;
  requestedCapabilities: readonly QueryOptionalCapability[];
}

export interface AcceptanceResult {
  accepted: boolean;
  recallImprovement: number;
  ndcgDelta: number;
  p95LatencyIncreaseRatio: number;
  newUnrequestedProviderCalls: number;
  checks: {
    hardConstraints: boolean;
    recall: boolean;
    ndcg: boolean;
    latency: boolean;
    providerCalls: boolean;
  };
  failures: string[];
}

export interface AcceptanceLimits {
  maxHardConstraintViolations: number;
  maxNdcgDrop: number;
  minRecallImprovement: number;
  maxP95LatencyIncreaseRatio: number;
  maxUnrequestedProviderCalls: number;
}

export const ACCEPTANCE_LIMITS = {
  maxHardConstraintViolations: 0,
  maxNdcgDrop: 0.005,
  minRecallImprovement: 0.05,
  maxP95LatencyIncreaseRatio: 0.25,
  maxUnrequestedProviderCalls: 0,
} satisfies AcceptanceLimits;

export function evaluateAcceptance(
  input: AcceptanceInput,
  limits: AcceptanceLimits = ACCEPTANCE_LIMITS,
): AcceptanceResult {
  const recallImprovement =
    input.targetRecallImprovement ??
    input.candidate.recallAt10 - input.baseline.recallAt10;
  const ndcgDelta = input.candidate.ndcgAt10 - input.baseline.ndcgAt10;
  const p95LatencyIncreaseRatio = latencyIncreaseRatio(
    input.baseline.p95LatencyMs,
    input.candidate.p95LatencyMs,
  );
  const newUnrequestedProviderCalls = unrequestedProviderCalls(input);
  const checks = {
    hardConstraints:
      input.candidate.hardConstraintViolationCount <=
      limits.maxHardConstraintViolations,
    recall: recallImprovement >= limits.minRecallImprovement,
    ndcg: ndcgDelta >= -limits.maxNdcgDrop,
    latency: p95LatencyIncreaseRatio <= limits.maxP95LatencyIncreaseRatio,
    providerCalls:
      newUnrequestedProviderCalls <= limits.maxUnrequestedProviderCalls,
  };
  const failures = Object.entries(checks)
    .filter(([, passed]) => !passed)
    .map(([name]) => name);
  return {
    accepted: failures.length === 0,
    recallImprovement,
    ndcgDelta,
    p95LatencyIncreaseRatio,
    newUnrequestedProviderCalls,
    checks,
    failures,
  };
}

function latencyIncreaseRatio(baseline: number, candidate: number): number {
  if (baseline === 0) return candidate === 0 ? 0 : Number.POSITIVE_INFINITY;
  return (candidate - baseline) / baseline;
}

function unrequestedProviderCalls(input: AcceptanceInput): number {
  const requested = new Set(input.requestedCapabilities);
  const increases = {
    semantic: positiveDelta(
      input.candidate.embeddingProviderCalls,
      input.baseline.embeddingProviderCalls,
    ),
    reranking: positiveDelta(
      input.candidate.rerankProviderCalls,
      input.baseline.rerankProviderCalls,
    ),
    associative: positiveDelta(
      input.candidate.enrichmentProviderCalls,
      input.baseline.enrichmentProviderCalls,
    ),
  } satisfies Record<"semantic" | "reranking" | "associative", number>;
  return Object.entries(increases).reduce(
    (total, [capability, calls]) =>
      total +
      (requested.has(capability as QueryOptionalCapability) ? 0 : calls),
    0,
  );
}

function positiveDelta(candidate: number, baseline: number): number {
  return Math.max(0, candidate - baseline);
}
