import type { NativeProviderWorkResult, NeedWork } from "../native/protocol.js";

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

export interface RerankScore {
  handle: string;
  score: number;
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

export type ProviderResult = Exclude<
  NativeProviderWorkResult,
  { type: "failure" }
>;

export class ProviderExecutionError extends Error {
  readonly providerType: ProviderType;
  readonly attempts: number;
  readonly retryable: boolean;
  readonly code: string;

  constructor(providerType: ProviderType, attempts: number, cause: unknown) {
    const metadata = providerErrorMetadata(cause);
    const reason = metadata.message === "" ? "" : `: ${metadata.message}`;
    super(
      `${providerType} provider failed after ${attempts} attempt(s)${reason}`,
      { cause },
    );
    this.name = "ProviderExecutionError";
    this.providerType = providerType;
    this.attempts = attempts;
    this.retryable = metadata.retryable;
    this.code = metadata.code;
  }
}

export function providerFailure(
  workId: string,
  error: unknown,
): Extract<NativeProviderWorkResult, { type: "failure" }> {
  const metadata = providerErrorMetadata(error);
  return {
    type: "failure",
    workId,
    retryable: metadata.retryable,
    code: metadata.code,
    message: metadata.message || "provider execution failed",
  };
}

function providerErrorMetadata(error: unknown): {
  retryable: boolean;
  code: string;
  message: string;
} {
  if (error instanceof ProviderExecutionError) {
    return {
      retryable: error.retryable,
      code: error.code,
      message: error.message,
    };
  }
  if (error && typeof error === "object") {
    const candidate = error as {
      retryable?: unknown;
      code?: unknown;
      message?: unknown;
    };
    return {
      retryable:
        typeof candidate.retryable === "boolean" ? candidate.retryable : false,
      code:
        typeof candidate.code === "string" && candidate.code.length > 0
          ? candidate.code
          : "PROVIDER_EXECUTION_FAILED",
      message:
        typeof candidate.message === "string"
          ? candidate.message
          : (JSON.stringify(error) ?? ""),
    };
  }
  return {
    retryable: false,
    code: "PROVIDER_EXECUTION_FAILED",
    message: error === undefined ? "" : (JSON.stringify(error) ?? ""),
  };
}
