import type { NeedWork } from "../native/protocol.js";

export type EmbeddingWork = Extract<NeedWork, { type: "embedding" }>;
export type RerankWork = Extract<NeedWork, { type: "rerank" }>;
export type EnrichmentWork = Extract<NeedWork, { type: "enrichment" }>;

export interface EmbeddingProvider {
  execute(work: EmbeddingWork, signal: AbortSignal): Promise<unknown>;
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

export interface ProviderResult {
  workId: string;
  accepted: boolean;
}
