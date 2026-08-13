import type { NeedWork } from "../native/protocol.js";
import type { ProviderResult, ProviderSet } from "./types.js";

export class ProviderHost {
  readonly #providers: ProviderSet;

  constructor(providers: ProviderSet) {
    this.#providers = providers;
  }

  async execute(work: NeedWork, signal: AbortSignal): Promise<ProviderResult> {
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
    return { workId: work.workId, accepted: true };
  }
}
