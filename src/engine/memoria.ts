import type {
  NativeFeedbackCommit,
  NativeFeedbackSubmission,
  NativeBinding,
  NativeCreateMemoryRequest,
  NativeMemoryMutation,
  NativeQueryRequest,
  NativeQueryResponse,
  NativePortableMemory,
  NativePurgePlan,
  NativeStatus,
  NativeStoreHandle,
} from "../native/protocol.js";
import { toNeedWork as decodeNeedWork } from "../native/protocol.js";
import { ProviderHost } from "../providers/host.js";
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
  #operations = new Set<string>();
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
      for (const operationId of this.#operations) {
        this.cancelNativeOperation(store, operationId);
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
    const normalized = normalizeQuery(query);
    this.assertNotAborted(options.signal);
    const store = this.store();
    const operationId = this.startOperation();
    const request: NativeQueryRequest = {
      scope: normalized.scope.spaces,
      ...(normalized.cue?.text === undefined
        ? {}
        : { text: normalized.cue.text }),
    };
    try {
      const response = await this.awaitOperation(
        operationId,
        Promise.resolve().then(() => this.#binding.queryStart(store, request)),
        options,
      );
      this.assertNotAborted(options.signal);
      return mapResponse(response);
    } catch (error) {
      throw toMemoriaError(error);
    } finally {
      this.#operations.delete(operationId);
    }
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

  async createSpace(spaceKey: string): Promise<string> {
    return this.#binding.authorityCreateSpace(this.store(), spaceKey);
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
    const normalized = normalizeQuery(query);
    const sessionId = this.#binding.readSessionOpen(this.store(), {
      scope: normalized.scope.spaces,
      ...(normalized.cue?.text === undefined
        ? {}
        : { text: normalized.cue.text }),
    });
    this.#readSessions.add(sessionId);
    return sessionId;
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
      let result;
      try {
        result = await this.#providerHost.execute(
          work,
          this.#providerAbort.signal,
        );
      } catch {
        result = { workId: work.workId, accepted: false };
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
    this.#operations.add(operationId);
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
        callback();
      };
      const onAbort = () => {
        this.cancelNativeOperation(store, operationId);
        finish(() =>
          reject(new MemoriaError("ABORTED", "The operation was aborted")),
        );
      };
      if (options.signal) {
        if (options.signal.aborted) {
          onAbort();
          return;
        }
        options.signal.addEventListener("abort", onAbort, { once: true });
      }
      if (options.timeoutMs !== undefined) {
        timer = setTimeout(() => {
          this.cancelNativeOperation(store, operationId);
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
