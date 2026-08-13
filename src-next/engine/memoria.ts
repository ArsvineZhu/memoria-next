import type {
  NativeBinding,
  NativeCreateMemoryRequest,
  NativeQueryRequest,
  NativeQueryResponse,
  NativeStatus,
  NativeStoreHandle,
} from "../native/protocol.js";
import { toNeedWork as decodeNeedWork } from "../native/protocol.js";
import { ProviderHost } from "../providers/host.js";

export interface MemoriaQuery {
  scope: string[];
  text?: string;
}

export interface MemoriaStatus {
  authorityGeneration: string;
  baseCoverage: string;
  semanticCoverage: string;
  activeReadLeases: number;
  closed: boolean;
}

export interface QueryOptions {
  signal?: AbortSignal;
}

export interface CreateMemoryRequest {
  spaceId: string;
  documentKey?: string;
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
  #binding: NativeBinding;
  #store: NativeStoreHandle | undefined;
  #providerHost: ProviderHost | undefined;
  #providerAbort = new AbortController();
  #wakeProviderPump: (() => void) | undefined;
  #closed = false;

  constructor(binding: NativeBinding, store: NativeStoreHandle, providerHost?: ProviderHost) {
    this.#binding = binding;
    this.#store = store;
    this.#providerHost = providerHost;
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
    this.#store = undefined;
    if (store) {
      this.#binding.closeStore(store);
    }
  }

  async status(): Promise<MemoriaStatus> {
    return mapStatus(this.store().status());
  }

  async query(query: MemoriaQuery, options: QueryOptions = {}): Promise<NativeQueryResponse> {
    this.assertNotAborted(options.signal);
    const request: NativeQueryRequest = {
      scope: query.scope,
      ...(query.text === undefined ? {} : { text: query.text }),
    };
    const response = this.#binding.queryStart(this.store(), request);
    this.assertNotAborted(options.signal);
    return mapResponse(response);
  }

  async createSpace(spaceKey: string): Promise<string> {
    return this.#binding.authorityCreateSpace(this.store(), spaceKey);
  }

  async createMemory(request: CreateMemoryRequest): Promise<CreatedMemory> {
    const nativeRequest: NativeCreateMemoryRequest = {
      spaceId: request.spaceId,
      ...(request.documentKey === undefined ? {} : { documentKey: request.documentKey }),
      mdx: request.mdx,
    };
    const memoryId = this.#binding.authorityMutate(this.store(), nativeRequest);
    this.#wakeProviderPump?.();
    const status = await this.status();
    return { memoryId, authorityGeneration: status.authorityGeneration };
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
        result = await this.#providerHost.execute(work, this.#providerAbort.signal);
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
      throw new Error("Memoria is closed");
    }
    return this.#store;
  }

  private assertNotAborted(signal: AbortSignal | undefined): void {
    if (signal?.aborted) {
      throw new DOMException("The operation was aborted", "AbortError");
    }
  }
}
