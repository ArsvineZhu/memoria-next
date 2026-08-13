import type { NeedWork } from "../native/protocol.js";
import {
  ProviderExecutionError,
  type EmbeddingPayload,
  type EmbeddingWork,
  type EnrichmentWork,
  type ProviderHostOptions,
  type ProviderResult,
  type ProviderSet,
  type RerankScore,
  type RerankWork,
} from "./types.js";

export class ProviderHost {
  readonly #providers: ProviderSet;
  readonly #maxAttempts: number;
  readonly #onDataEgress: ProviderHostOptions["onDataEgress"];

  constructor(options: ProviderSet | ProviderHostOptions) {
    if ("providers" in options) {
      this.#providers = options.providers;
      this.#maxAttempts = options.maxAttempts ?? 1;
      this.#onDataEgress = options.onDataEgress;
    } else {
      this.#providers = options;
      this.#maxAttempts = 1;
      this.#onDataEgress = undefined;
    }
    if (!Number.isSafeInteger(this.#maxAttempts) || this.#maxAttempts < 1) {
      throw new Error("provider maxAttempts must be a positive integer");
    }
  }

  async execute(work: NeedWork, signal: AbortSignal): Promise<ProviderResult> {
    const egressWork = this.#onDataEgress
      ? await this.#onDataEgress(work)
      : work;
    const providerType = egressWork.type;
    let lastError: unknown;
    for (let attempt = 1; attempt <= this.#maxAttempts; attempt += 1) {
      if (signal.aborted) {
        throw new DOMException("The operation was aborted", "AbortError");
      }
      try {
        const providerResult = await this.executeOnce(egressWork, signal);
        const embeddings =
          egressWork.type === "embedding"
            ? validateEmbeddingPayload(egressWork, providerResult)
            : undefined;
        const scores =
          egressWork.type === "rerank"
            ? validateRerankPayload(egressWork, providerResult)
            : undefined;
        const tags =
          egressWork.type === "enrichment"
            ? validateTagEnrichmentPayload(egressWork, providerResult)
            : undefined;
        return {
          workId: work.workId,
          accepted: true,
          embeddings,
          scores,
          tags,
        };
      } catch (error) {
        lastError = error;
        if (signal.aborted || attempt === this.#maxAttempts) {
          throw new ProviderExecutionError(providerType, attempt, error);
        }
      }
    }
    throw new ProviderExecutionError(
      providerType,
      this.#maxAttempts,
      lastError,
    );
  }

  private async executeOnce(
    work: NeedWork,
    signal: AbortSignal,
  ): Promise<unknown> {
    switch (work.type) {
      case "embedding": {
        const provider = this.#providers.embedding;
        if (!provider) {
          throw new Error("embedding provider is not configured");
        }
        return provider.execute(work, signal);
      }
      case "rerank": {
        const provider = this.#providers.rerank;
        if (!provider) {
          throw new Error("rerank provider is not configured");
        }
        return provider.execute(work, signal);
      }
      case "enrichment": {
        const provider = this.#providers.enrichment;
        if (!provider) {
          throw new Error("enrichment provider is not configured");
        }
        return provider.execute(work, signal);
      }
    }
  }
}

function validateEmbeddingPayload(
  work: EmbeddingWork,
  result: unknown,
): EmbeddingPayload[] {
  const vectors = Array.isArray(result)
    ? result
    : result && typeof result === "object" && "vectors" in result
      ? (result as { vectors: unknown }).vectors
      : undefined;
  if (!Array.isArray(vectors) || vectors.length !== work.items.length) {
    throw new Error("embedding provider returned the wrong item count");
  }
  return vectors.map((vector, index) => {
    const values = Array.isArray(vector)
      ? vector
      : vector && typeof vector === "object" && "values" in vector
        ? (vector as { values: unknown }).values
        : undefined;
    if (
      !Array.isArray(values) ||
      values.length !== work.dimensions ||
      values.some(
        (value) => typeof value !== "number" || !Number.isFinite(value),
      )
    ) {
      throw new Error(
        `embedding dimension/value validation failed for item ${work.items[index]?.key ?? index}`,
      );
    }
    return { key: work.items[index].key, values: [...values] };
  });
}

function validateRerankPayload(
  work: RerankWork,
  result: unknown,
): RerankScore[] {
  const scores = Array.isArray(result)
    ? result
    : result && typeof result === "object" && "scores" in result
      ? (result as { scores: unknown }).scores
      : undefined;
  if (!Array.isArray(scores) || scores.length !== work.candidates.length) {
    throw new Error("rerank provider returned the wrong score count");
  }
  const expected = new Set(work.candidates);
  const seen = new Set<string>();
  return scores.map((item, index) => {
    if (
      !item ||
      typeof item !== "object" ||
      !("handle" in item) ||
      !("score" in item)
    ) {
      throw new Error(`rerank score ${index} must contain a handle and score`);
    }
    const handle = (item as { handle: unknown }).handle;
    const score = (item as { score: unknown }).score;
    if (
      typeof handle !== "string" ||
      handle.length === 0 ||
      !expected.has(handle) ||
      seen.has(handle)
    ) {
      throw new Error(
        `rerank score ${index} returned an unknown or duplicate handle`,
      );
    }
    if (
      typeof score !== "number" ||
      !Number.isFinite(score) ||
      score < 0 ||
      score > 1
    ) {
      throw new Error(
        `rerank score ${index} must be finite and between 0 and 1`,
      );
    }
    seen.add(handle);
    return { handle, score };
  });
}

function validateTagEnrichmentPayload(
  work: EnrichmentWork,
  result: unknown,
): string[] {
  const tags = Array.isArray(result)
    ? result
    : result && typeof result === "object" && "tags" in result
      ? (result as { tags: unknown }).tags
      : undefined;
  if (!Array.isArray(tags) || tags.length > work.projection.maxTags) {
    throw new Error("tag enrichment provider returned too many candidates");
  }
  return tags.map((tag, index) => {
    if (typeof tag !== "string" || tag.trim().length === 0) {
      throw new Error(
        `tag enrichment candidate ${index} must be a non-empty string`,
      );
    }
    return tag.trim();
  });
}
