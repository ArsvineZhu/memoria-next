import { readdir, rm } from "node:fs/promises";
import { join } from "node:path";

import type {
  NativeBinding,
  NativeCreateMemoryRequest,
  NativeFeedbackCommit,
  NativeFeedbackSubmission,
  NativeMemoryMutation,
  NativePortableMemory,
  NativeProviderResult,
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
  resolve: (value: unknown) => void;
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
    calls,
    async execute(work) {
      calls.push({
        dimensions: work.dimensions,
        items: work.items.map((item) => ({ key: item.key, text: item.text })),
      });
      return new Promise<unknown>((resolve, reject) => {
        pending.push({
          resolve,
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
        item.resolve({ vectors: [values] });
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
  providerSubmissions: ObservedProviderResult[];
}

export interface ObservedProviderResult extends NativeProviderResult {
  readonly embeddings?: Array<{ key: string; values: number[] }>;
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
  const providerSubmissions: ObservedProviderResult[] = [];
  const providerWork = [...(options.providerWork ?? [])];
  const status: NativeStatus = {
    authorityGeneration: "0",
    baseCoverage: "0",
    semanticCoverage: "0",
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
        result: NativeProviderResult,
      ) {
        providerSubmissions.push({
          workId: result.workId,
          accepted: result.accepted,
          scores: result.scores,
          tags: result.tags,
        });
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
  predicate: () => boolean,
  timeoutMs = 1_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (!predicate() && Date.now() < deadline) {
    await new Promise<void>((resolve) => setTimeout(resolve, 1));
  }
  if (!predicate()) {
    throw new Error("condition did not become true before timeout");
  }
}

export async function removeAuthoritySourceObjects(
  dataDir: string,
): Promise<number> {
  // Leave Authority SQLite intact; only remove immutable source CAS objects.
  const authorityObjectsDir = join(dataDir, "authority", "objects");
  const entries = await readdir(authorityObjectsDir, { withFileTypes: true });
  let removed = 0;
  for (const entry of entries) {
    if (!entry.isFile()) {
      continue;
    }
    await rm(join(authorityObjectsDir, entry.name), { force: true });
    removed += 1;
  }
  return removed;
}
