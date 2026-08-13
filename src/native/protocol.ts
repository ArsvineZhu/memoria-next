export interface NativeQueryRequest {
  scope: string[];
  text?: string;
}

export interface NativeQueryResponse {
  resultCount: number;
  authorityGeneration: string;
  degraded: boolean;
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
}

export interface NativeProviderItem {
  key: string;
  text: string;
}

export interface NativeProviderWork {
  workId: string;
  workType: string;
  signature: string;
  items: NativeProviderItem[];
  query?: string;
  candidates: string[];
  text?: string;
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
  readSessionOpen(store: NativeStoreHandle, request: NativeQueryRequest): string;
  readSessionClose(store: NativeStoreHandle, sessionId: string): void;
  cancelOperation(store: NativeStoreHandle, operationId: string): void;
}

export type NeedWork =
  | {
      type: "embedding";
      workId: string;
      signature: string;
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
      text: string;
    };

export function toNeedWork(work: NativeProviderWork): NeedWork {
  switch (work.workType) {
    case "embedding":
      return {
        type: "embedding",
        workId: work.workId,
        signature: work.signature,
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
      return {
        type: "enrichment",
        workId: work.workId,
        signature: work.signature,
        text: work.text ?? "",
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
