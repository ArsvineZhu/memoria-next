import type {
  NativeBinding,
  NativeCreateMemoryRequest,
  NativeFeedbackCommit,
  NativeFeedbackSubmission,
  NativeMemoryMutation,
  NativePortableMemory,
  NativeProviderWorkResult,
  NativeProviderWork,
  NativePurgePlan,
  NativeQueryRequest,
  NativeQueryResponse,
  NativeReviseMemoryRequest,
  NativeStatus,
  NativeStoreHandle,
} from "../../src/native/protocol.js";
import type { EmbeddingProvider } from "../../src/providers/types.js";

interface PendingEmbeddingResolution {
  resolve: (values: number[]) => void;
  reject: (error: unknown) => void;
}

export interface DeferredObservedEmbeddingProvider extends EmbeddingProvider {
  readonly calls: Array<{
    dimensions: number;
    items: Array<{ key: string; text: string }>;
  }>;
  callCount(): number;
  pendingCount(): number;
  resolveAll(values?: number[]): void;
  rejectAll(error?: Error): void;
}

export function deferredObservedEmbeddingProvider(
  defaultValues: number[] = [0.25, 0.5, 0.25],
): DeferredObservedEmbeddingProvider {
  const calls: DeferredObservedEmbeddingProvider["calls"] = [];
  const pending: PendingEmbeddingResolution[] = [];
  return {
    trust: "external",
    calls,
    async execute(work) {
      calls.push({
        dimensions: work.dimensions,
        items: work.items.map((item) => ({ key: item.key, text: item.text })),
      });
      return new Promise<unknown>((resolve, reject) => {
        pending.push({
          resolve: (values) =>
            resolve({
              vectors: [
                {
                  key: work.items[0]?.key ?? "query",
                  values: [...values],
                },
              ],
            }),
          reject,
        });
      });
    },
    callCount() {
      return calls.length;
    },
    pendingCount() {
      return pending.length;
    },
    resolveAll(values = defaultValues) {
      const work = pending.splice(0);
      for (const item of work) {
        item.resolve(values);
      }
    },
    rejectAll(error = new Error("deferred embedding provider rejected")) {
      const work = pending.splice(0);
      for (const item of work) {
        item.reject(error);
      }
    },
  };
}

export interface BindingHarness {
  binding: NativeBinding;
  queryRequests: NativeQueryRequest[];
  providerSubmissions: NativeProviderWorkResult[];
}

interface BindingHarnessOptions {
  queryResponse?: NativeQueryResponse;
  providerWork?: NativeProviderWork[];
  status?: Partial<NativeStatus>;
}

export function createBindingHarness(
  options: BindingHarnessOptions = {},
): BindingHarness {
  const queryRequests: NativeQueryRequest[] = [];
  const providerSubmissions: NativeProviderWorkResult[] = [];
  const providerWork = [...(options.providerWork ?? [])];
  const status: NativeStatus = {
    authorityGeneration: "0",
    baseCoverage: "0",
    semanticCoverage: "0",
    semanticBuildCoverage: "0",
    activeReadLeases: 0,
    closed: false,
    ...options.status,
  };
  const store: NativeStoreHandle = {
    close() {
      status.closed = true;
    },
    status() {
      return status;
    },
  };
  const defaultQueryResponse: NativeQueryResponse = {
    resultCount: 0,
    authorityGeneration: status.authorityGeneration,
    degraded: false,
    retrievalId: "RET_remediation",
    results: [],
  };
  const queryResponse = options.queryResponse ?? defaultQueryResponse;
  const defaultMutation: NativeMemoryMutation = {
    memoryId: "M_remediation",
    spaceId: "SP_remediation",
    revisionId: "R_remediation",
    authorityGeneration: "1",
  };
  const defaultPurgePlan: NativePurgePlan = {
    id: "PURGE_remediation",
    memoryId: "M_remediation",
    state: "planned",
  };
  const defaultFeedback: NativeFeedbackCommit = {
    generation: "0",
    events: [],
  };
  const defaultPortableMemories: NativePortableMemory[] = [];
  return {
    queryRequests,
    providerSubmissions,
    binding: {
      openStore() {
        return store;
      },
      closeStore(value) {
        value.close();
      },
      authorityCreateSpace() {
        return "SP_remediation";
      },
      authorityMutate(
        _store: NativeStoreHandle,
        _request: NativeCreateMemoryRequest,
      ) {
        return defaultMutation.memoryId;
      },
      authorityRevise(
        _store: NativeStoreHandle,
        _request: NativeReviseMemoryRequest,
      ) {
        return defaultMutation;
      },
      exportMemories() {
        return defaultPortableMemories;
      },
      purgePlan() {
        return defaultPurgePlan;
      },
      purgeExecute() {
        return { ...defaultPurgePlan, state: "completed" };
      },
      queryStart(_store: NativeStoreHandle, request: NativeQueryRequest) {
        queryRequests.push(request);
        return queryResponse;
      },
      queryResume() {
        return queryResponse;
      },
      providerPollWork() {
        return providerWork.shift() ?? null;
      },
      providerSubmitResult(
        _store: NativeStoreHandle,
        result: NativeProviderWorkResult,
      ) {
        // Observe the exact object crossing the TypeScript NativeBinding boundary.
        providerSubmissions.push(result);
      },
      feedbackSubmit(
        _store: NativeStoreHandle,
        _request: NativeFeedbackSubmission,
      ) {
        return defaultFeedback;
      },
      readSessionOpen() {
        return "RS_remediation";
      },
      readSessionClose() {},
      cancelOperation() {},
    },
  };
}

export async function waitFor(
  predicate: () => boolean | Promise<boolean>,
  timeoutMs = 1_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let satisfied = await predicate();
  while (!satisfied && Date.now() < deadline) {
    await new Promise<void>((resolve) => setTimeout(resolve, 1));
    satisfied = await predicate();
  }
  if (!satisfied) {
    throw new Error("condition did not become true before timeout");
  }
}
