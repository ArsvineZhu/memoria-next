import type { RerankScore } from "../providers/types.js";
import { ProviderHost } from "../providers/host.js";

export interface ScopedRerankCandidate {
  handle: string;
  spaceId: string;
  text: string;
  score: number;
  rerankScore?: number;
}

export interface ScopedRerankOptions {
  query: string;
  scope: readonly string[];
  candidates: readonly ScopedRerankCandidate[];
  provider: ProviderHost;
  maxCandidates?: number;
  signal?: AbortSignal;
}

const RERANK_SIGNATURE = "memoria-rerank-v1";
const DEFAULT_MAX_CANDIDATES = 32;

/**
 * Apply one optional rerank barrier to candidates that already passed scope
 * filtering. Provider output can only reorder those handles; it cannot add a
 * candidate or restore one excluded by the scope boundary.
 */
export async function rerankScopedCandidates(
  options: ScopedRerankOptions,
): Promise<ScopedRerankCandidate[]> {
  const maxCandidates = options.maxCandidates ?? DEFAULT_MAX_CANDIDATES;
  if (!Number.isSafeInteger(maxCandidates) || maxCandidates < 1) {
    throw new Error("rerank maxCandidates must be a positive integer");
  }
  const scope = new Set(options.scope);
  const inScope = options.candidates
    .filter((candidate) => scope.has(candidate.spaceId))
    .map(validateCandidate)
    .filter(
      (candidate, index, candidates) =>
        candidates.findIndex((item) => item.handle === candidate.handle) ===
        index,
    )
    .sort(compareBaseCandidates);
  if (inScope.length === 0) {
    return [];
  }

  const selected = inScope.slice(0, maxCandidates);
  const result = await options.provider.execute(
    {
      type: "rerank",
      workId: `RERANK_${selected.length}`,
      signature: RERANK_SIGNATURE,
      query: options.query,
      candidates: selected.map((candidate) => candidate.handle),
    },
    options.signal ?? new AbortController().signal,
  );
  if (result.type !== "rerank") {
    throw new Error("rerank provider returned a non-rerank result");
  }
  const scores = result.scores;
  const byHandle = new Map(scores.map((score) => [score.handle, score]));
  if (
    scores.length !== selected.length ||
    selected.some((candidate) => !byHandle.has(candidate.handle))
  ) {
    throw new Error(
      "rerank provider result did not cover the requested handles",
    );
  }

  const reranked = selected
    .map((candidate) => ({
      ...candidate,
      rerankScore: byHandle.get(candidate.handle)?.score,
    }))
    .sort(compareRerankedCandidates);
  return [...reranked, ...inScope.slice(maxCandidates)];
}

function validateCandidate(
  candidate: ScopedRerankCandidate,
): ScopedRerankCandidate {
  if (candidate.handle.trim().length === 0) {
    throw new Error("rerank candidate handle must be non-empty");
  }
  if (!Number.isFinite(candidate.score)) {
    throw new Error(
      `rerank candidate ${candidate.handle} has an invalid score`,
    );
  }
  return candidate;
}

function compareBaseCandidates(
  left: ScopedRerankCandidate,
  right: ScopedRerankCandidate,
): number {
  return right.score - left.score || left.handle.localeCompare(right.handle);
}

function compareRerankedCandidates(
  left: ScopedRerankCandidate & { rerankScore: number | undefined },
  right: ScopedRerankCandidate & { rerankScore: number | undefined },
): number {
  return (
    (right.rerankScore ?? -1) - (left.rerankScore ?? -1) ||
    compareBaseCandidates(left, right)
  );
}

export type { RerankScore };
