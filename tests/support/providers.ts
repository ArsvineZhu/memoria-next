import type { EmbeddingProvider } from "../../src/providers/types.js";

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
              vectors: [Array.from({ length: work.dimensions }, () => 0)],
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
