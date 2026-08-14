import { loadNativeBinding } from "../native/binding.js";
import type { NativeBinding, NativeStoreHandle } from "../native/protocol.js";
import {
  ProviderHost,
  createProviderEgressGuard,
  type ProviderEgressPolicy,
} from "../providers/host.js";
import { validateMemoriaConfig, type MemoriaConfig } from "../domain/config.js";
import { toMemoriaError } from "../domain/errors.js";
import { Memoria } from "./memoria.js";

export type CreateMemoriaOptions = MemoriaConfig & { binding?: NativeBinding };

export async function createMemoria(
  options: CreateMemoriaOptions,
): Promise<Memoria> {
  const { binding: injectedBinding, ...publicConfig } = options;
  const config = validateMemoriaConfig(publicConfig);
  const binding = injectedBinding ?? loadNativeBinding();
  let store: NativeStoreHandle;
  try {
    store = binding.openStore(config.dataDir);
  } catch (error) {
    throw toMemoriaError(error);
  }
  const privacy = config.privacy;
  const providerEgressPolicy: ProviderEgressPolicy = {
    embedding:
      privacy?.allowProviderDataEgress !== false &&
      privacy?.allowEmbeddingDataEgress !== false,
    rerank:
      privacy?.allowProviderDataEgress !== false &&
      privacy?.allowRerankDataEgress !== false,
    enrichment:
      privacy?.allowProviderDataEgress !== false &&
      privacy?.allowEnrichmentDataEgress !== false,
  };
  return new Memoria(
    binding,
    store,
    config.providers
      ? new ProviderHost({
          providers: config.providers,
          onDataEgress: createProviderEgressGuard(providerEgressPolicy),
        })
      : undefined,
    config.dataDir,
    config.resourceLimits,
  );
}
