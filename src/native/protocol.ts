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
  activeReadLeases: number;
  closed: boolean;
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

export interface NativeMemoryMutation {
  memoryId: string;
  spaceId: string;
  revisionId: string;
  authorityGeneration: string;
}

export interface NativeProviderResult {
  workId: string;
  accepted: boolean;
  scores?: Array<{ handle: string; score: number }>;
  tags?: string[];
}

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
}

export interface NativeStoreHandle {
  close(): void;
  status(): NativeStatus;
}

export interface NativeBinding {
  openStore(dataDir: string): NativeStoreHandle;
  closeStore(store: NativeStoreHandle): void;
  authorityMutate(store: NativeStoreHandle, request: NativeCreateMemoryRequest): string;
  authorityCreateSpace(store: NativeStoreHandle, spaceKey: string): string;
  authorityRevise(store: NativeStoreHandle, request: NativeReviseMemoryRequest): NativeMemoryMutation;
  exportMemories(store: NativeStoreHandle, scope: string[]): NativePortableMemory[];
  purgePlan(store: NativeStoreHandle, memoryId: string): NativePurgePlan;
  purgeExecute(store: NativeStoreHandle, planId: string): NativePurgePlan;
  queryStart(
    store: NativeStoreHandle,
    request: NativeQueryRequest,
  ): NativeQueryResponse | Promise<NativeQueryResponse>;
  queryResume(
    store: NativeStoreHandle,
    operationId: string,
  ): NativeQueryResponse | Promise<NativeQueryResponse>;
  providerPollWork(store: NativeStoreHandle): NativeProviderWork | null;
  providerSubmitResult(store: NativeStoreHandle, result: NativeProviderResult): void;
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
    }
  | {
      type: "rerank";
      workId: string;
      signature: string;
      query: string;
      candidates: string[];
  }
  | {
      type: "enrichment";
      workId: string;
      signature: string;
      projection: NativeEnrichmentProjection;
    };

export function toNeedWork(work: NativeProviderWork): NeedWork {
  switch (work.workType) {
    case "embedding":
      return {
        type: "embedding",
        workId: work.workId,
        signature: work.signature,
        dimensions: work.dimensions,
        items: work.items,
      };
    case "rerank":
      return {
        type: "rerank",
        workId: work.workId,
        signature: work.signature,
        query: work.query ?? "",
        candidates: work.candidates,
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
      };
    default:
      throw new Error(`Unsupported native provider work type: ${work.workType}`);
  }
}

export type QueryStartResult =
  | { type: "final"; response: NativeQueryResponse }
  | { type: "need-work"; operationId: string; work: NeedWork };

export function asQueryStartResult(result: NativeQueryResponse | QueryStartResult): QueryStartResult {
  if ("type" in result && (result.type === "final" || result.type === "need-work")) {
    return result;
  }
  return { type: "final", response: result };
}
