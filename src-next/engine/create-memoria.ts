import { loadNativeBinding } from "../native/binding.js";
import type { NativeBinding } from "../native/protocol.js";
import { ProviderHost } from "../providers/host.js";
import { validateMemoriaConfig, type MemoriaConfig } from "../domain/config.js";
import { Memoria } from "./memoria.js";

export type CreateMemoriaOptions = MemoriaConfig & { binding?: NativeBinding };

export async function createMemoria(options: CreateMemoriaOptions): Promise<Memoria> {
  const { binding: injectedBinding, ...publicConfig } = options;
  const config = validateMemoriaConfig(publicConfig);
  const binding = injectedBinding ?? loadNativeBinding();
  const store = binding.openStore(config.dataDir);
  return new Memoria(binding, store, config.providers ? new ProviderHost(config.providers) : undefined);
}
