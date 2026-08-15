import type { NeedWork } from "../native/protocol.js";
import pRetry from "p-retry";
import { MemoriaError } from "../domain/errors.js";
import {
  ProviderExecutionError,
  isRetryableProviderError,
  type ProviderTrust,
  type EmbeddingPayload,
  type EmbeddingWork,
  type EnrichmentWork,
  type ProviderHostOptions,
  type ProviderResult,
  type ProviderSet,
  type RerankScore,
  type RerankWork,
  providerRouteSignature,
} from "./types.js";

const PROVIDER_RETRY_OPTIONS = {
  retries: 4,
  factor: 2,
  minTimeout: 500,
  maxTimeout: 8_000,
  randomize: true,
  maxRetryTime: 30_000,
} as const;

export interface ProviderEgressPolicy {
  embedding: boolean;
  rerank: boolean;
  enrichment: boolean;
}

export function createProviderEgressGuard(
  policy: ProviderEgressPolicy,
): NonNullable<ProviderHostOptions["onDataEgress"]> {
  return (work) => {
    if (!policy[work.type]) {
      throw new MemoriaError(
        "CAPABILITY_NOT_READY",
        "CAPABILITY_NOT_READY: " +
          work.type +
          " provider data egress is denied",
      );
    }
    return work;
  };
}

export class ProviderHost {
  readonly #providers: ProviderSet;
  readonly #onDataEgress: ProviderHostOptions["onDataEgress"];

  constructor(options: ProviderSet | ProviderHostOptions) {
    if ("providers" in options) {
      this.#providers = options.providers;
      this.#onDataEgress = options.onDataEgress;
    } else {
      this.#providers = options;
      this.#onDataEgress = undefined;
    }
  }

  async execute(work: NeedWork, signal: AbortSignal): Promise<ProviderResult> {
    assertProviderTrust(work, providerTrust(this.#providers, work.type));
    const egressWork = this.#onDataEgress
      ? await this.#onDataEgress(work)
      : work;
    assertProviderTrust(
      egressWork,
      providerTrust(this.#providers, egressWork.type),
    );
    const providerType = egressWork.type;
    let attempts = 0;
    try {
      return await pRetry(
        async () => {
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
          switch (egressWork.type) {
            case "embedding":
              return {
                type: "embeddings",
                workId: work.workId,
                vectors: embeddings!,
              };
            case "rerank":
              return { type: "rerank", workId: work.workId, scores: scores! };
            case "enrichment":
              return {
                type: "enrichment",
                workId: work.workId,
                tags: tags!,
              };
          }
        },
        {
          ...PROVIDER_RETRY_OPTIONS,
          signal,
          onFailedAttempt: ({ attemptNumber }) => {
            attempts = attemptNumber;
          },
          shouldRetry: ({ error }) => isRetryableProviderError(error),
        },
      );
    } catch (error) {
      if (signal.aborted) {
        throw (
          signal.reason ??
          new DOMException("The operation was aborted", "AbortError")
        );
      }
      throw new ProviderExecutionError(providerType, attempts || 1, error);
    }
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

function providerTrust(
  providers: ProviderSet,
  providerType: NeedWork["type"],
): ProviderTrust {
  const provider = providers[providerType];
  return provider?.trust ?? "external";
}

function assertProviderTrust(work: NeedWork, trust: ProviderTrust): void {
  const route = work.route;
  if (
    route &&
    (route.capability !== work.type ||
      route.trust !== trust ||
      route.signature !== providerRouteSignature(work.type))
  ) {
    throw new MemoriaError(
      "PROVIDER_POLICY_DENIED",
      `PROVIDER_POLICY_DENIED: ${work.type} provider route metadata does not match the configured provider`,
    );
  }
  const mode =
    work.spacePolicy?.[work.type === "rerank" ? "reranking" : work.type] ??
    "external-allowed";
  const permitted =
    mode === "external-allowed" || (mode === "local-only" && trust === "local");
  if (!permitted) {
    throw new MemoriaError(
      "PROVIDER_POLICY_DENIED",
      `PROVIDER_POLICY_DENIED: ${work.type} provider route (${trust}) is not permitted by the scoped Space policy`,
    );
  }
}

function validateEmbeddingPayload(
  work: EmbeddingWork,
  result: unknown,
): EmbeddingPayload[] {
  const vectors =
    result && typeof result === "object" && "vectors" in result
      ? (result as { vectors: unknown }).vectors
      : undefined;
  if (!Array.isArray(vectors) || vectors.length !== work.items.length) {
    throw new Error("embedding provider returned the wrong item count");
  }
  const expected = new Set(work.items.map((item) => item.key));
  const seen = new Set<string>();
  const byKey = new Map<string, number[]>();
  vectors.forEach((vector, index) => {
    if (!vector || typeof vector !== "object" || !("key" in vector)) {
      throw new Error(`embedding vector ${index} is missing its key`);
    }
    const key = (vector as { key: unknown }).key;
    const values =
      "values" in vector ? (vector as { values: unknown }).values : undefined;
    if (
      typeof key !== "string" ||
      key.length === 0 ||
      !expected.has(key) ||
      seen.has(key)
    ) {
      throw new Error(
        `embedding vector ${index} returned an unknown or duplicate key`,
      );
    }
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
    seen.add(key);
    byKey.set(key, [...values]);
  });
  if (seen.size !== expected.size) {
    throw new Error("embedding provider result is missing a requested key");
  }
  return work.items.map((item) => ({
    key: item.key,
    values: byKey.get(item.key)!,
  }));
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
