import { ProviderHost } from "../../src/providers/host.js";
import type {
  EmbeddingProvider,
  EnrichmentWork,
  TagEnrichmentProvider,
} from "../../src/providers/types.js";

interface PendingEmbedding {
  resolve: (value: unknown) => void;
  reject: (error: unknown) => void;
}

export interface DeferredEmbeddingProvider extends EmbeddingProvider {
  pendingCount(): number;
  resolveAll(): void;
  rejectAll(error?: Error): void;
}

export function deferredEmbeddingProvider(): DeferredEmbeddingProvider {
  const pending: PendingEmbedding[] = [];
  return {
    execute(work, _signal) {
      return new Promise<unknown>((resolve, reject) => {
        pending.push({
          resolve: () =>
            resolve({
              vectors: [
                {
                  key: work.items[0]?.key ?? "query",
                  values: Array.from({ length: work.dimensions }, () => 0),
                },
              ],
            }),
          reject,
        });
      });
    },
    pendingCount() {
      return pending.length;
    },
    resolveAll() {
      const work = pending.splice(0);
      for (const item of work) {
        item.resolve(undefined);
      }
    },
    rejectAll(error = new Error("deferred provider rejected")) {
      const work = pending.splice(0);
      for (const item of work) {
        item.reject(error);
      }
    },
  };
}

export async function waitForPendingProvider(
  provider: DeferredEmbeddingProvider,
  timeoutMs = 1_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (provider.pendingCount() === 0 && Date.now() < deadline) {
    await new Promise<void>((resolve) => setTimeout(resolve, 1));
  }
  if (provider.pendingCount() === 0) {
    throw new Error("provider work did not become pending before timeout");
  }
}

export async function waitForSemanticCoverage(
  memoria: { status(): Promise<{ semanticCoverage: string }> },
  targetGeneration: string,
  timeoutMs = 1_000,
): Promise<void> {
  const target = BigInt(targetGeneration);
  const deadline = Date.now() + timeoutMs;
  while (
    BigInt((await memoria.status()).semanticCoverage) < target &&
    Date.now() < deadline
  ) {
    await new Promise<void>((resolve) => setTimeout(resolve, 1));
  }
  if (BigInt((await memoria.status()).semanticCoverage) < target) {
    throw new Error(
      `semantic coverage did not reach ${targetGeneration} before timeout`,
    );
  }
}

export function tagEnrichmentWork(workId: string): EnrichmentWork {
  return {
    type: "enrichment",
    workId,
    signature: "tag-enrichment-v1",
    projection: {
      version: 1,
      inputHash: "00".repeat(32),
      spaceId: "SP_AAAAAAAAAAAAAAAAAAAAAAAAAA",
      memoryId: "M_AAAAAAAAAAAAAAAAAAAAAAAAA",
      revisionId: "R_".concat("00".repeat(32)),
      semanticNodeId: undefined,
      content: "Career Rust systems work",
      maxTags: 8,
    },
  };
}

export function providerHostWithTagEnrichment(tags: string[]): ProviderHost {
  return new ProviderHost({
    enrichment: {
      async execute() {
        return { tags };
      },
    },
  });
}

export interface ObservedTagEnrichmentProvider extends TagEnrichmentProvider {
  works: EnrichmentWork[];
}

export function observedTagEnrichmentProvider(
  tags: string[],
): ObservedTagEnrichmentProvider {
  const works: EnrichmentWork[] = [];
  return {
    works,
    async execute(work) {
      works.push(work);
      return { tags };
    },
  };
}

export async function waitForTagEnrichment(
  provider: ObservedTagEnrichmentProvider,
  timeoutMs = 1_000,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (provider.works.length === 0 && Date.now() < deadline) {
    await new Promise<void>((resolve) => setTimeout(resolve, 1));
  }
  if (provider.works.length === 0) {
    throw new Error("tag enrichment work did not reach the provider");
  }
}
