import { loadNativeBinding } from "../native/binding.js";
import type { NativeBinding } from "../native/protocol.js";
import { ProviderHost } from "../providers/host.js";
import type { ProviderSet } from "../providers/types.js";
import { Memoria } from "./memoria.js";

export interface CreateMemoriaOptions {
  dataDir: string;
  binding?: NativeBinding;
  providers?: ProviderSet;
}

export async function createMemoria(options: CreateMemoriaOptions): Promise<Memoria> {
  const binding = options.binding ?? loadNativeBinding();
  const store = binding.openStore(options.dataDir);
  return new Memoria(binding, store, options.providers ? new ProviderHost(options.providers) : undefined);
}
