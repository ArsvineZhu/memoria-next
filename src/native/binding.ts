import { createRequire } from "node:module";
import { resolve } from "node:path";

import type { NativeBinding } from "./protocol.js";

const require = createRequire(import.meta.url);

export function loadNativeBinding(): NativeBinding {
  return require(resolve(process.cwd(), "native", "index.js")) as NativeBinding;
}
