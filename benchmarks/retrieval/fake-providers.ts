import type {
  EmbeddingWork,
  ProviderSet,
  RerankWork,
} from "../../src/providers/types.js";

export interface ProviderCounts {
  embedding: number;
  rerank: number;
  enrichment: number;
}

export interface DeterministicProviderOptions {
  generatedTagsByMemoryId?: ReadonlyMap<string, readonly string[]>;
}

export const FIXED_EMBEDDING_DIMENSIONS = 64;

export function createDeterministicProviders(
  counts: ProviderCounts,
  options: DeterministicProviderOptions = {},
): ProviderSet {
  return {
    embedding: {
      trust: "local",
      async execute(work: EmbeddingWork) {
        counts.embedding += 1;
        return {
          vectors: work.items.map((item) => ({
            key: item.key,
            values: embeddingForWork(work, item),
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
      async execute(work) {
        counts.enrichment += 1;
        return [
          ...(options.generatedTagsByMemoryId?.get(work.projection.memoryId) ??
            []),
        ];
      },
    },
  };
}

function embeddingForWork(
  work: EmbeddingWork,
  item: { key: string; text: string },
): number[] {
  const seed = `${item.key}\u0000${item.text}`;
  const values = Array.from(
    { length: FIXED_EMBEDDING_DIMENSIONS },
    (_, index) => {
      const hash = stableHash(`${seed}\u0000${index}`);
      return (hash / 0xffffffff) * 2 - 1;
    },
  );
  const norm = Math.sqrt(values.reduce((sum, value) => sum + value * value, 0));
  const normalized = values.map((value) => value / (norm || 1));

  if (work.dimensions !== FIXED_EMBEDDING_DIMENSIONS) {
    throw new Error(
      `benchmark embedding contract expected ${FIXED_EMBEDDING_DIMENSIONS} dimensions, got ${work.dimensions}`,
    );
  }
  const finalNorm = Math.sqrt(
    normalized.reduce((sum, value) => sum + value * value, 0),
  );
  if (Math.abs(finalNorm - 1) > 1e-6) {
    throw new Error("benchmark embedding vector is not L2-normalized");
  }
  return normalized;
}

function rerankScore(query: string, handle: string, index: number): number {
  const hash = stableHash(query + "\u0000" + handle);
  const positionBonus = Math.max(0, 1 - index / 64);
  return Math.min(1, 0.75 * ((hash % 10_000) / 10_000) + 0.25 * positionBonus);
}

export function stableHash(value: string): number {
  let hash = 2_166_136_261;
  for (const character of value) {
    hash ^= character.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 16_777_619);
  }
  return hash >>> 0;
}
