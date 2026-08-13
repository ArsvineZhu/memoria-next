import type { NeedWork } from "../native/protocol.js";

export type EmbeddingWork = Extract<NeedWork, { type: "embedding" }>;
export type RerankWork = Extract<NeedWork, { type: "rerank" }>;
export type EnrichmentWork = Extract<NeedWork, { type: "enrichment" }>;

export interface EmbeddingProvider {
  execute(work: EmbeddingWork, signal: AbortSignal): Promise<unknown>;
}

export interface EmbeddingPayload {
  key: string;
  values: number[];
}

export interface RerankProvider {
  execute(work: RerankWork, signal: AbortSignal): Promise<unknown>;
}

export interface TagEnrichmentProvider {
  execute(work: EnrichmentWork, signal: AbortSignal): Promise<unknown>;
}

export interface ProviderSet {
  embedding?: EmbeddingProvider;
  rerank?: RerankProvider;
  enrichment?: TagEnrichmentProvider;
}

export type ProviderType = "embedding" | "rerank" | "enrichment";

export type ProviderEgressHook = (
  work: NeedWork,
) => NeedWork | Promise<NeedWork>;

export interface ProviderHostOptions {
  providers: ProviderSet;
  maxAttempts?: number;
  onDataEgress?: ProviderEgressHook;
}

export interface ProviderResult {
  workId: string;
  accepted: boolean;
  embeddings?: EmbeddingPayload[];
}

export class ProviderExecutionError extends Error {
  readonly providerType: ProviderType;
  readonly attempts: number;

  constructor(providerType: ProviderType, attempts: number, cause: unknown) {
    const reason = cause instanceof Error ? `: ${cause.message}` : "";
    super(
      `${providerType} provider failed after ${attempts} attempt(s)${reason}`,
      { cause },
    );
    this.name = "ProviderExecutionError";
    this.providerType = providerType;
    this.attempts = attempts;
  }
}
