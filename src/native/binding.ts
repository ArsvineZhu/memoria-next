import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import type { NativeBinding } from "./protocol.js";

const require = createRequire(import.meta.url);

export function loadNativeBinding(): NativeBinding {
  const moduleDirectory = dirname(fileURLToPath(import.meta.url));
  const packageRoot = [
    resolve(moduleDirectory, "..", ".."),
    resolve(moduleDirectory, "..", "..", ".."),
  ].find((candidate) => existsSync(resolve(candidate, "native", "index.js")));
  if (!packageRoot) {
    throw new Error("NATIVE_ERROR: installed native loader was not found");
  }
  return require(resolve(packageRoot, "native", "index.js")) as NativeBinding;
}
