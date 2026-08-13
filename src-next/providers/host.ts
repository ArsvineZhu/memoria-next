import type { NeedWork } from "../native/protocol.js";
import {
  ProviderExecutionError,
  type ProviderHostOptions,
  type ProviderResult,
  type ProviderSet,
  type ProviderType,
} from "./types.js";

export class ProviderHost {
  readonly #providers: ProviderSet;
  readonly #maxAttempts: number;
  readonly #onDataEgress: ProviderHostOptions["onDataEgress"];

  constructor(options: ProviderSet | ProviderHostOptions) {
    if ("providers" in options) {
      this.#providers = options.providers;
      this.#maxAttempts = options.maxAttempts ?? 1;
      this.#onDataEgress = options.onDataEgress;
    } else {
      this.#providers = options;
      this.#maxAttempts = 1;
      this.#onDataEgress = undefined;
    }
    if (!Number.isSafeInteger(this.#maxAttempts) || this.#maxAttempts < 1) {
      throw new Error("provider maxAttempts must be a positive integer");
    }
  }

  async execute(work: NeedWork, signal: AbortSignal): Promise<ProviderResult> {
    const egressWork = this.#onDataEgress ? await this.#onDataEgress(work) : work;
    const providerType = egressWork.type;
    let lastError: unknown;
    for (let attempt = 1; attempt <= this.#maxAttempts; attempt += 1) {
      if (signal.aborted) {
        throw new DOMException("The operation was aborted", "AbortError");
      }
      try {
        await this.executeOnce(egressWork, signal);
        return { workId: work.workId, accepted: true };
      } catch (error) {
        lastError = error;
        if (signal.aborted || attempt === this.#maxAttempts) {
          throw new ProviderExecutionError(providerType, attempt, error);
        }
      }
    }
    throw new ProviderExecutionError(providerType, this.#maxAttempts, lastError);
  }

  private async executeOnce(work: NeedWork, signal: AbortSignal): Promise<void> {
    switch (work.type) {
      case "embedding": {
        const provider = this.#providers.embedding;
        if (!provider) {
          throw new Error("embedding provider is not configured");
        }
        await provider.execute(work, signal);
        break;
      }
      case "rerank": {
        const provider = this.#providers.rerank;
        if (!provider) {
          throw new Error("rerank provider is not configured");
        }
        await provider.execute(work, signal);
        break;
      }
      case "enrichment": {
        const provider = this.#providers.enrichment;
        if (!provider) {
          throw new Error("enrichment provider is not configured");
        }
        await provider.execute(work, signal);
        break;
      }
    }
  }
}
