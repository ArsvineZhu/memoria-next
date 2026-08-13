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
  mdx: string;
}

export interface NativeProviderResult {
  workId: string;
  accepted: boolean;
}

export interface NativeStoreHandle {
  close(): void;
  status(): NativeStatus;
}

export interface NativeBinding {
  openStore(dataDir: string): NativeStoreHandle;
  closeStore(store: NativeStoreHandle): void;
  authorityMutate(store: NativeStoreHandle, request: NativeCreateMemoryRequest): string;
  queryStart(store: NativeStoreHandle, request: NativeQueryRequest): NativeQueryResponse;
  queryResume(store: NativeStoreHandle, operationId: string): NativeQueryResponse;
  providerPollWork(store: NativeStoreHandle): string | null;
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
      candidates: Array<{ key: string; text: string }>;
    }
  | {
      type: "enrichment";
      workId: string;
      signature: string;
      items: Array<{ key: string; text: string }>;
    };

export type QueryStartResult =
  | { type: "final"; response: NativeQueryResponse }
  | { type: "need-work"; operationId: string; work: NeedWork };

export function asQueryStartResult(result: NativeQueryResponse | QueryStartResult): QueryStartResult {
  if ("type" in result && (result.type === "final" || result.type === "need-work")) {
    return result;
  }
  return { type: "final", response: result };
}
