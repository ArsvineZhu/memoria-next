import type { NeedWork } from "../native/protocol.js";
import {
  ProviderExecutionError,
  type EmbeddingPayload,
  type EmbeddingWork,
  type ProviderHostOptions,
  type ProviderResult,
  type ProviderSet,
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
        return { workId: work.workId, accepted: true, embeddings };
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
