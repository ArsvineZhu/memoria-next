export interface NativeQueryMemoryReference {
  memoryId: string;
  revisionId?: string;
  nodeId?: string;
}

export interface NativeQueryCue {
  text?: string;
  tags?: string[];
  entities?: string[];
  memories?: NativeQueryMemoryReference[];
}

export interface NativeQueryConstraints {
  tags?: string[];
  entities?: string[];
  memories?: NativeQueryMemoryReference[];
  lifecycle?: "active" | "retired" | "any";
}

export interface NativeQueryTemporal {
  validAt?: string;
}

export interface NativeQueryHistory {
  mode: "current" | "all-revisions" | "changes";
  fromAuthorityGeneration?: string;
  toAuthorityGeneration?: string;
}

export type NativeQueryAuthorityConsistency =
  | { mode: "latest" }
  | { mode: "at-least"; generation: string }
  | { mode: "exact"; generation: string };

export interface NativeQueryConsistency {
  authority: NativeQueryAuthorityConsistency;
  required: string[];
  preferred: string[];
  onNotReady: "fail" | "wait";
  timeoutMs: number;
}

export interface NativeQueryBudget {
  maxResults: number;
  maxMatchesPerResult: number;
  maxEvidenceTokens: number;
}

export interface NativeQueryRequest {
  scope: string[];
  cue?: NativeQueryCue;
  constraints?: NativeQueryConstraints;
  temporal?: NativeQueryTemporal;
  history: NativeQueryHistory;
  consistency: NativeQueryConsistency;
  budget: NativeQueryBudget;
  quality: "fast" | "balanced" | "thorough";
}

export interface NativeQueryResponse {
  resultCount: number;
  authorityGeneration: string;
  degraded: boolean;
  retrievalId: string;
  results: NativeQueryResult[];
}

export interface NativeQueryResult {
  resultId: string;
  spaceId: string;
  memoryId: string;
  revisionId: string;
}

export interface NativeStatus {
  authorityGeneration: string;
  baseCoverage: string;
  semanticCoverage: string;
  semanticBuildCoverage: string;
  activeReadLeases: number;
  closed: boolean;
}

export interface NativeBackupResult {
  path: string;
  storeId: string;
  authorityGeneration: string;
  includeAdaptive: boolean;
  fileCount: number;
  sourceObjectCount: number;
  manifestHash: string;
}

export interface NativeCreateMemoryRequest {
  spaceId: string;
  documentKey?: string;
  idempotencyKey?: string;
  mdx: string;
}

export interface NativeReviseMemoryRequest {
  memoryId: string;
  expectedHead: string;
  mdx: string;
}

export type NativeSpaceProviderMode =
  | "deny"
  | "local-only"
  | "external-allowed";

export interface NativeSpaceProviderPolicy {
  embedding: NativeSpaceProviderMode;
  reranking: NativeSpaceProviderMode;
  enrichment: NativeSpaceProviderMode;
}

export interface NativeMemoryMutation {
  memoryId: string;
  spaceId: string;
  revisionId: string;
  authorityGeneration: string;
}

export type NativeProviderWorkResult =
  | {
      type: "embeddings";
      workId: string;
      vectors: Array<{ key: string; values: number[] }>;
    }
  | {
      type: "rerank";
      workId: string;
      scores: Array<{ handle: string; score: number }>;
    }
  | {
      type: "enrichment";
      workId: string;
      tags: string[];
    }
  | {
      type: "failure";
      workId: string;
      retryable: boolean;
      code: string;
      message: string;
    };

export interface NativeFeedbackSubmission {
  retrievalId: string;
  idempotencyKey: string;
  events: Array<{ resultId: string; outcome: string }>;
}

export interface NativeFeedbackCommit {
  generation: string;
  events: Array<{
    eventId: string;
    generation: string;
    retrievalId: string;
    spaceId: string;
    memoryId: string;
    revisionId: string;
    semanticNodeId?: string;
    outcome: string;
    occurredAtSeconds: number;
  }>;
}

export interface NativePortableMemory {
  sourceId: string;
  spaceId: string;
  revisionId: string;
  mdx: string;
}

export interface NativePortableImportRequest {
  targetSpaceKey: string;
  idempotencyKey: string;
  requestFingerprint: string;
  originStoreId?: string;
  memories: NativePortableMemory[];
}

export interface NativePortableImportResult {
  targetSpaceId: string;
  mappings: Array<{ sourceId: string; targetId: string }>;
  unresolvedExternalReferences: string[];
}

export interface NativePurgePlan {
  id: string;
  memoryId: string;
  state: "planned" | "committed" | "cleaning" | "completed";
}

export interface NativeProviderItem {
  key: string;
  text: string;
}

export interface NativeEnrichmentProjection {
  version: number;
  inputHash: string;
  spaceId: string;
  memoryId: string;
  revisionId: string;
  semanticNodeId?: string;
  content: string;
  maxTags: number;
}

export interface NativeProviderWork {
  workId: string;
  workType: string;
  signature: string;
  dimensions: number;
  items: NativeProviderItem[];
  query?: string;
  candidates: string[];
  projection?: NativeEnrichmentProjection;
  spacePolicy?: NativeSpaceProviderPolicy;
}

export type NativeQueryWork =
  | {
      type: "query-embedding";
      workId: string;
      signature: string;
      input: { key: string; text: string };
      spacePolicy?: NativeSpaceProviderPolicy;
    }
  | {
      type: "query-rerank";
      workId: string;
      signature: string;
      query: string;
      candidates: Array<{ handle: string; text: string }>;
      spacePolicy?: NativeSpaceProviderPolicy;
    };

export type NativeQueryStep =
  | { state: "complete"; response: NativeQueryResponse }
  | { state: "pending"; operationId: string; work: NativeQueryWork };

export interface NativeStoreHandle {
  close(): void;
  status(): NativeStatus;
}

export interface NativeBinding {
  openStore(dataDir: string): NativeStoreHandle;
  closeStore(store: NativeStoreHandle): void;
  authorityMutate(store: NativeStoreHandle, request: NativeCreateMemoryRequest): string;
  authorityCreateSpace(
    store: NativeStoreHandle,
    spaceKey: string,
    providerPolicy?: NativeSpaceProviderPolicy,
  ): string;
  authorityRevise(store: NativeStoreHandle, request: NativeReviseMemoryRequest): NativeMemoryMutation;
  exportMemories(store: NativeStoreHandle, scope: string[]): NativePortableMemory[];
  importPortable?(
    store: NativeStoreHandle,
    request: NativePortableImportRequest,
  ): NativePortableImportResult;
  backupCreate?(
    store: NativeStoreHandle,
    outputPath: string | undefined,
    includeAdaptive: boolean,
  ): NativeBackupResult;
  backupRestore?(backupPath: string, targetPath: string): NativeBackupResult;
  purgePlan(store: NativeStoreHandle, memoryId: string): NativePurgePlan;
  purgeExecute(store: NativeStoreHandle, planId: string): NativePurgePlan;
  queryStart(
    store: NativeStoreHandle,
    request: NativeQueryRequest,
  ): NativeQueryResponse | NativeQueryStep | Promise<NativeQueryResponse | NativeQueryStep>;
  queryResume(
    store: NativeStoreHandle,
    operationId: string,
    result: NativeProviderWorkResult,
  ): NativeQueryResponse | NativeQueryStep | Promise<NativeQueryResponse | NativeQueryStep>;
  providerPollWork(store: NativeStoreHandle): NativeProviderWork | null;
  providerSubmitResult(store: NativeStoreHandle, result: NativeProviderWorkResult): void;
  feedbackSubmit(
    store: NativeStoreHandle,
    request: NativeFeedbackSubmission,
  ): NativeFeedbackCommit;
  readSessionOpen(store: NativeStoreHandle, request: NativeQueryRequest): string;
  readSessionClose(store: NativeStoreHandle, sessionId: string): void;
  cancelOperation(store: NativeStoreHandle, operationId: string): void;
}

export type NeedWork =
  | {
      type: "embedding";
      workId: string;
      signature: string;
      dimensions: number;
      items: Array<{ key: string; text: string }>;
      spacePolicy?: NativeSpaceProviderPolicy;
    }
  | {
      type: "rerank";
      workId: string;
      signature: string;
      query: string;
      candidates: string[];
      spacePolicy?: NativeSpaceProviderPolicy;
  }
  | {
      type: "enrichment";
      workId: string;
      signature: string;
      projection: NativeEnrichmentProjection;
      spacePolicy?: NativeSpaceProviderPolicy;
    };

export function toNeedWork(work: NativeProviderWork | NativeQueryWork): NeedWork {
  if ("workType" in work) {
    switch (work.workType) {
      case "embedding":
        return {
          type: "embedding",
          workId: work.workId,
          signature: work.signature,
          dimensions: work.dimensions,
          items: work.items,
          spacePolicy: work.spacePolicy,
        };
      case "rerank":
        return {
          type: "rerank",
          workId: work.workId,
          signature: work.signature,
          query: work.query ?? "",
          candidates: work.candidates,
          spacePolicy: work.spacePolicy,
        };
      case "enrichment":
        if (!work.projection) {
          throw new Error("Enrichment provider work is missing its projection");
        }
        return {
          type: "enrichment",
          workId: work.workId,
          signature: work.signature,
          projection: work.projection,
          spacePolicy: work.spacePolicy,
        };
      default:
        throw new Error(`Unsupported native provider work type: ${work.workType}`);
    }
  }
  switch (work.type) {
    case "query-embedding":
      return {
        type: "embedding",
        workId: work.workId,
        signature: work.signature,
        dimensions: 3,
        items: [work.input],
        spacePolicy: work.spacePolicy,
      };
    case "query-rerank":
      return {
        type: "rerank",
        workId: work.workId,
        signature: work.signature,
        query: work.query,
        candidates: work.candidates.map((candidate) => candidate.handle),
        spacePolicy: work.spacePolicy,
      };
  }
}

export function asQueryStep(
  result: NativeQueryResponse | NativeQueryStep,
): NativeQueryStep {
  if ("state" in result && (result.state === "complete" || result.state === "pending")) {
    return result;
  }
  return { state: "complete", response: result };
}
