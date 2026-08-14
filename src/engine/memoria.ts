import type {
  NativeFeedbackCommit,
  NativeFeedbackSubmission,
  NativeBackupResult,
  NativeBinding,
  NativeCreateMemoryRequest,
  NativeMemoryMutation,
  NativePortableImportRequest,
  NativePortableImportResult,
  NativeProviderWorkResult,
  NativeQueryRequest,
  NativeQueryResponse,
  NativePortableMemory,
  NativePurgePlan,
  NativeSpaceProviderPolicy,
  NativeStatus,
  NativeStoreHandle,
} from "../native/protocol.js";
import {
  asQueryStep,
  toNeedWork as decodeNeedWork,
} from "../native/protocol.js";
import { ProviderHost } from "../providers/host.js";
import { providerFailure } from "../providers/types.js";
import { createAdminApi, type AdminApi } from "../admin/index.js";
import { createDocumentsApi, type DocumentsApi } from "../domain/documents.js";
import { MemoriaError, toMemoriaError } from "../domain/errors.js";
import {
  createFeedbackApi,
  type FeedbackApi,
  type FeedbackInput,
} from "../domain/feedback.js";
import { createSpacesApi, type SpacesApi } from "../domain/spaces.js";
import type { ResourceLimits } from "../domain/config.js";
import { normalizeQuery, type MemoryQueryInput } from "../domain/query.js";

export interface MemoriaStatus {
  authorityGeneration: string;
  baseCoverage: string;
  semanticCoverage: string;
  semanticBuildCoverage: string;
  activeReadLeases: number;
  closed: boolean;
}

export interface QueryOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
}

export interface CreateMemoryRequest {
  spaceId: string;
  documentKey?: string;
  idempotencyKey?: string;
  mdx: string;
}

export interface CreatedMemory {
  memoryId: string;
  authorityGeneration: string;
}

function mapStatus(status: NativeStatus): MemoriaStatus {
  return {
    authorityGeneration: status.authorityGeneration,
    baseCoverage: status.baseCoverage,
    semanticCoverage: status.semanticCoverage,
    semanticBuildCoverage: status.semanticBuildCoverage,
    activeReadLeases: status.activeReadLeases,
    closed: status.closed,
  };
}

function mapResponse(response: NativeQueryResponse): NativeQueryResponse {
  return response;
}

export class Memoria {
  readonly spaces: SpacesApi;
  readonly documents: DocumentsApi;
  readonly feedback: FeedbackApi;
  readonly admin: AdminApi;
  #binding: NativeBinding;
  #store: NativeStoreHandle | undefined;
  #providerHost: ProviderHost | undefined;
  #providerAbort = new AbortController();
  #wakeProviderPump: (() => void) | undefined;
  #nextOperation = 0;
  #operations = new Map<string, string | undefined>();
  #readSessions = new Set<string>();
  #closed = false;
  #resourceLimits: ResourceLimits;

  constructor(
    binding: NativeBinding,
    store: NativeStoreHandle,
    providerHost?: ProviderHost,
    dataDir = "",
    resourceLimits: ResourceLimits = {},
  ) {
    this.#binding = binding;
    this.#store = store;
    this.#providerHost = providerHost;
    this.#resourceLimits = resourceLimits;
    this.spaces = createSpacesApi(this);
    this.documents = createDocumentsApi(this);
    this.feedback = createFeedbackApi(this);
    this.admin = createAdminApi(this, dataDir);
    if (providerHost) {
      void this.runProviderPump();
    }
  }

  async close(): Promise<void> {
    if (this.#closed) {
      return;
    }
    const store = this.#store;
    this.#closed = true;
    this.#providerAbort.abort();
    this.#wakeProviderPump?.();
    this.#wakeProviderPump = undefined;
    if (store) {
      for (const nativeOperationId of this.#operations.values()) {
        if (nativeOperationId !== undefined) {
          this.cancelNativeOperation(store, nativeOperationId);
        }
      }
      for (const sessionId of this.#readSessions) {
        try {
          this.#binding.readSessionClose(store, sessionId);
        } catch {
          // Store close is the final lease boundary; native cleanup is best effort here.
        }
      }
    }
    this.#operations.clear();
    this.#readSessions.clear();
    this.#store = undefined;
    if (store) {
      this.#binding.closeStore(store);
    }
  }

  async status(): Promise<MemoriaStatus> {
    try {
      const status = mapStatus(this.store().status());
      return {
        ...status,
        activeReadLeases: Math.max(
          status.activeReadLeases,
          this.#readSessions.size,
        ),
      };
    } catch (error) {
      throw toMemoriaError(error);
    }
  }

  async query(
    query: MemoryQueryInput,
    options: QueryOptions = {},
  ): Promise<NativeQueryResponse> {
    let operationId: string | undefined;
    try {
      const normalized = normalizeQuery(query);
      this.assertNotAborted(options.signal);
      operationId = this.startOperation();
      const request: NativeQueryRequest = {
        scope: normalized.scope.spaces,
        ...(normalized.cue === undefined ? {} : { cue: normalized.cue }),
        ...(normalized.constraints === undefined
          ? {}
          : { constraints: normalized.constraints }),
        ...(normalized.temporal === undefined
          ? {}
          : { temporal: normalized.temporal }),
        history: normalized.history,
        consistency: normalized.consistency,
        budget: normalized.budget,
        quality: normalized.quality,
      };
      const response = await this.awaitOperation(
        operationId,
        this.runQuerySteps(operationId, request, options.signal),
        options,
      );
      this.assertNotAborted(options.signal);
      return mapResponse(response);
    } catch (error) {
      throw toMemoriaError(error);
    } finally {
      if (operationId !== undefined) {
        this.#operations.delete(operationId);
      }
    }
  }

  private async runQuerySteps(
    operationId: string,
    request: NativeQueryRequest,
    signal: AbortSignal | undefined,
  ): Promise<NativeQueryResponse> {
    let step = asQueryStep(
      await this.#binding.queryStart(this.store(), request),
    );
    const providerSignal = signal
      ? AbortSignal.any([signal, this.#providerAbort.signal])
      : this.#providerAbort.signal;
    while (step.state === "pending") {
      this.#operations.set(operationId, step.operationId);
      this.assertNotAborted(signal);
      const work = decodeNeedWork(step.work);
      let result: NativeProviderWorkResult;
      const providerHost = this.#providerHost;
      if (!providerHost) {
        throw new MemoriaError(
          "CAPABILITY_NOT_READY",
          "CAPABILITY_NOT_READY: query provider is not configured",
        );
      }
      try {
        result = await providerHost.execute(work, providerSignal);
      } catch (error) {
        if (providerSignal.aborted) {
          throw new MemoriaError("ABORTED", "The operation was aborted");
        }
        result = providerFailure(work.workId, error);
      }
      if (providerSignal.aborted) {
        throw new MemoriaError("ABORTED", "The operation was aborted");
      }
      step = asQueryStep(
        await this.#binding.queryResume(this.store(), step.operationId, result),
      );
    }
    return mapResponse(step.response);
  }

  async submitFeedback(input: FeedbackInput): Promise<NativeFeedbackCommit> {
    const request: NativeFeedbackSubmission = {
      retrievalId: input.retrievalId,
      idempotencyKey: input.idempotencyKey,
      events: input.events,
    };
    try {
      return this.#binding.feedbackSubmit(this.store(), request);
    } catch (error) {
      throw toMemoriaError(error);
    }
  }

  async createSpace(
    spaceKey: string,
    providerPolicy?: NativeSpaceProviderPolicy,
  ): Promise<string> {
    return this.#binding.authorityCreateSpace(
      this.store(),
      spaceKey,
      providerPolicy,
    );
  }

  async createMemory(request: CreateMemoryRequest): Promise<CreatedMemory> {
    this.assertSourceSize(request.mdx);
    const nativeRequest: NativeCreateMemoryRequest = {
      spaceId: request.spaceId,
      ...(request.documentKey === undefined
        ? {}
        : { documentKey: request.documentKey }),
      ...(request.idempotencyKey === undefined
        ? {}
        : { idempotencyKey: request.idempotencyKey }),
      mdx: request.mdx,
    };
    const memoryId = this.#binding.authorityMutate(this.store(), nativeRequest);
    this.#wakeProviderPump?.();
    const status = await this.status();
    return { memoryId, authorityGeneration: status.authorityGeneration };
  }

  async exportMemories(scope: string[]): Promise<NativePortableMemory[]> {
    return this.#binding.exportMemories(this.store(), scope);
  }

  async importPortable(
    request: NativePortableImportRequest,
  ): Promise<NativePortableImportResult> {
    if (!this.#binding.importPortable) {
      throw new MemoriaError(
        "UNSUPPORTED_OPERATION",
        "UNSUPPORTED_OPERATION: native portable import support is unavailable",
      );
    }
    const result = this.#binding.importPortable(this.store(), request);
    this.#wakeProviderPump?.();
    return result;
  }

  async createBackup(
    options: {
      includeAdaptive?: boolean;
      outputPath?: string;
    } = {},
  ): Promise<NativeBackupResult> {
    if (!this.#binding.backupCreate) {
      throw new MemoriaError(
        "UNSUPPORTED_OPERATION",
        "UNSUPPORTED_OPERATION: native backup support is unavailable",
      );
    }
    return this.#binding.backupCreate(
      this.store(),
      options.outputPath,
      options.includeAdaptive === true,
    );
  }

  async restoreBackup(input: {
    backupPath: string;
    targetDir: string;
  }): Promise<NativeBackupResult> {
    if (!this.#binding.backupRestore) {
      throw new MemoriaError(
        "UNSUPPORTED_OPERATION",
        "UNSUPPORTED_OPERATION: native restore support is unavailable",
      );
    }
    return this.#binding.backupRestore(input.backupPath, input.targetDir);
  }

  async planPurge(memoryId: string): Promise<NativePurgePlan> {
    return this.#binding.purgePlan(this.store(), memoryId);
  }

  async executePurge(planId: string): Promise<NativePurgePlan> {
    return this.#binding.purgeExecute(this.store(), planId);
  }

  async reviseMemory(request: {
    memoryId: string;
    expectedHead: string;
    mdx: string;
  }): Promise<NativeMemoryMutation> {
    this.assertSourceSize(request.mdx);
    const mutation = this.#binding.authorityRevise(this.store(), request);
    this.#wakeProviderPump?.();
    return mutation;
  }

  async openReadSession(query: MemoryQueryInput): Promise<string> {
    try {
      const normalized = normalizeQuery(query);
      const sessionId = this.#binding.readSessionOpen(this.store(), {
        scope: normalized.scope.spaces,
        ...(normalized.cue === undefined ? {} : { cue: normalized.cue }),
        ...(normalized.constraints === undefined
          ? {}
          : { constraints: normalized.constraints }),
        ...(normalized.temporal === undefined
          ? {}
          : { temporal: normalized.temporal }),
        history: normalized.history,
        consistency: normalized.consistency,
        budget: normalized.budget,
        quality: normalized.quality,
      });
      this.#readSessions.add(sessionId);
      return sessionId;
    } catch (error) {
      throw toMemoriaError(error);
    }
  }

  async closeReadSession(sessionId: string): Promise<void> {
    const store = this.store();
    this.#binding.readSessionClose(store, sessionId);
    this.#readSessions.delete(sessionId);
  }

  private async runProviderPump(): Promise<void> {
    while (!this.#closed && this.#providerHost) {
      const store = this.#store;
      if (!store) {
        return;
      }
      const nativeWork = this.#binding.providerPollWork(store);
      if (!nativeWork) {
        await new Promise<void>((resolve) => {
          this.#wakeProviderPump = resolve;
        });
        this.#wakeProviderPump = undefined;
        continue;
      }

      const work = decodeNeedWork(nativeWork);
      let result: NativeProviderWorkResult;
      try {
        result = await this.#providerHost.execute(
          work,
          this.#providerAbort.signal,
        );
      } catch (error) {
        result = providerFailure(work.workId, error);
      }
      if (!this.#closed && this.#store) {
        this.#binding.providerSubmitResult(this.#store, result);
      }
    }
  }

  private store(): NativeStoreHandle {
    if (this.#closed || !this.#store) {
      throw new MemoriaError("STORE_CLOSED", "Memoria is closed");
    }
    return this.#store;
  }

  private assertSourceSize(source: string): void {
    const maximum = this.#resourceLimits.maxSourceBytes;
    if (maximum !== undefined && Buffer.byteLength(source, "utf8") > maximum) {
      throw new MemoriaError(
        "RESOURCE_LIMIT",
        "RESOURCE_LIMIT: source bytes exceed configured maximum",
      );
    }
  }

  private assertNotAborted(signal: AbortSignal | undefined): void {
    if (signal?.aborted) {
      throw new MemoriaError("ABORTED", "The operation was aborted");
    }
  }

  private startOperation(): string {
    this.#nextOperation += 1;
    const operationId = `OP_${this.#nextOperation}`;
    this.#operations.set(operationId, undefined);
    return operationId;
  }

  private cancelNativeOperation(
    store: NativeStoreHandle,
    operationId: string,
  ): void {
    try {
      this.#binding.cancelOperation(store, operationId);
    } catch {
      // Cancellation is best effort after a terminal close boundary.
    }
  }

  private cancelOperationHandle(
    store: NativeStoreHandle,
    operationHandle: string,
  ): void {
    const nativeOperationId = this.#operations.get(operationHandle);
    this.cancelNativeOperation(store, nativeOperationId ?? operationHandle);
  }

  private awaitOperation<T>(
    operationId: string,
    operation: Promise<T>,
    options: QueryOptions,
  ): Promise<T> {
    if (
      options.timeoutMs !== undefined &&
      (!Number.isSafeInteger(options.timeoutMs) || options.timeoutMs < 1)
    ) {
      return Promise.reject(
        new MemoriaError(
          "QUERY_TIMEOUT",
          "timeoutMs must be a positive integer",
        ),
      );
    }
    const store = this.store();
    return new Promise<T>((resolve, reject) => {
      let settled = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const finish = (callback: () => void) => {
        if (settled) {
          return;
        }
        settled = true;
        if (timer) {
          clearTimeout(timer);
        }
        options.signal?.removeEventListener("abort", onAbort);
        this.#providerAbort.signal.removeEventListener("abort", onClose);
        callback();
      };
      const onAbort = () => {
        this.cancelOperationHandle(store, operationId);
        finish(() =>
          reject(new MemoriaError("ABORTED", "The operation was aborted")),
        );
      };
      const onClose = () => {
        this.cancelOperationHandle(store, operationId);
        finish(() =>
          reject(new MemoriaError("STORE_CLOSED", "The store was closed")),
        );
      };
      if (options.signal) {
        if (options.signal.aborted) {
          onAbort();
          return;
        }
        options.signal.addEventListener("abort", onAbort, { once: true });
      }
      if (this.#providerAbort.signal.aborted) {
        onClose();
        return;
      }
      this.#providerAbort.signal.addEventListener("abort", onClose, {
        once: true,
      });
      if (options.timeoutMs !== undefined) {
        timer = setTimeout(() => {
          this.cancelOperationHandle(store, operationId);
          finish(() =>
            reject(new MemoriaError("QUERY_TIMEOUT", "The query timed out")),
          );
        }, options.timeoutMs);
      }
      operation.then(
        (value) => finish(() => resolve(value)),
        (error) => finish(() => reject(error)),
      );
    });
  }
}
