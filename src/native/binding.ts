import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import type { NativeBinding } from "./protocol.js";

const require = createRequire(import.meta.url);

export function loadNativeBinding(): NativeBinding {
  const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..", "..");
  return require(resolve(packageRoot, "native", "index.js")) as NativeBinding;
}
