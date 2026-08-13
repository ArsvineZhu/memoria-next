import type {
  NativeBinding,
  NativeQueryRequest,
  NativeQueryResponse,
  NativeStatus,
  NativeStoreHandle,
} from "../native/protocol.js";

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
  #closed = false;

  constructor(binding: NativeBinding, store: NativeStoreHandle) {
    this.#binding = binding;
    this.#store = store;
  }

  async close(): Promise<void> {
    if (this.#closed) {
      return;
    }
    const store = this.#store;
    this.#closed = true;
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
