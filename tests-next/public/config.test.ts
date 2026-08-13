import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src-next/engine/create-memoria.js";
import { isMemoriaError } from "../../src-next/domain/errors.js";
import type { MemoriaConfig } from "../../src-next/domain/config.js";

test("public config rejects physical planner knobs", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-config-"));
  try {
    await assert.rejects(
      () =>
        createMemoria({
          dataDir,
          // @ts-expect-error physical planner knob is not public API
          diffusionSteps: 4,
        }),
      (error: unknown) => isMemoriaError(error, "UNSUPPORTED_OPERATION"),
    );
  } finally {
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("provider concurrency and readiness timeout are operational config", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-config-"));
  const config: MemoriaConfig = {
    dataDir,
    runtime: {
      providerConcurrency: 4,
      defaultReadinessTimeoutMs: 2_000,
    },
  };
  const memoria = await createMemoria(config);
  await memoria.close();
  await rm(dataDir, { recursive: true, force: true });
});
