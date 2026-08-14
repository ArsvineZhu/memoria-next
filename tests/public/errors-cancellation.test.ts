import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { isMemoriaError } from "../../src/domain/errors.js";
import { asSpaceId } from "../../src/domain/ids.js";
import type {
  NativeBinding,
  NativeCreateMemoryRequest,
  NativeMemoryMutation,
  NativeProviderResult,
  NativeQueryRequest,
  NativeQueryResponse,
  NativeReviseMemoryRequest,
  NativeStatus,
  NativeStoreHandle,
} from "../../src/native/protocol.js";

function fakeBinding(queryStart: NativeBinding["queryStart"]): NativeBinding {
  const status: NativeStatus = {
    authorityGeneration: "0",
    baseCoverage: "0",
    semanticCoverage: "0",
    activeReadLeases: 0,
    closed: false,
  };
  const store: NativeStoreHandle = {
    close() {
      status.closed = true;
    },
    status() {
      return status;
    },
  };
  return {
    openStore() {
      return store;
    },
    closeStore(value) {
      value.close();
    },
    authorityCreateSpace() {
      return "SP_fake";
    },
    authorityMutate(
      _value: NativeStoreHandle,
      _request: NativeCreateMemoryRequest,
    ) {
      return "M_fake";
    },
    authorityRevise(
      _value: NativeStoreHandle,
      _request: NativeReviseMemoryRequest,
    ): NativeMemoryMutation {
      return {
        memoryId: "M_fake",
        spaceId: "SP_fake",
        revisionId: "R_fake",
        authorityGeneration: "1",
      };
    },
    exportMemories() {
      return [];
    },
    purgePlan() {
      return {
        id: "PURGE_fake",
        memoryId: "M_fake",
        state: "planned" as const,
      };
    },
    purgeExecute() {
      return {
        id: "PURGE_fake",
        memoryId: "M_fake",
        state: "completed" as const,
      };
    },
    queryStart,
    queryResume() {
      return {
        resultCount: 0,
        authorityGeneration: "0",
        degraded: false,
        retrievalId: "RET_fake",
        results: [],
      };
    },
    providerPollWork() {
      return null;
    },
    providerSubmitResult(
      _value: NativeStoreHandle,
      _result: NativeProviderResult,
    ) {},
    feedbackSubmit() {
      return { generation: "0", events: [] };
    },
    readSessionOpen() {
      return "RS_fake";
    },
    readSessionClose() {},
    cancelOperation() {},
  };
}

function neverCompletes(
  _store: NativeStoreHandle,
  _request: NativeQueryRequest,
): Promise<NativeQueryResponse> {
  return new Promise<NativeQueryResponse>(() => {});
}

test("cancelled query returns ABORTED and releases its snapshot lease", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-cancel-"));
  const memoria = await createMemoria({
    dataDir,
    binding: fakeBinding(neverCompletes),
  });
  try {
    const controller = new AbortController();
    const promise = memoria.query(
      { scope: { spaces: [asSpaceId("SP_fake")] } },
      { signal: controller.signal },
    );
    controller.abort();

    await assert.rejects(
      () => promise,
      (error: unknown) => isMemoriaError(error, "ABORTED"),
    );
    assert.equal((await memoria.status()).activeReadLeases, 0);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("query timeout is distinct from capability-not-ready", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-timeout-"));
  const memoria = await createMemoria({
    dataDir,
    binding: fakeBinding(neverCompletes),
  });
  try {
    await assert.rejects(
      () =>
        memoria.query(
          { scope: { spaces: [asSpaceId("SP_fake")] } },
          { timeoutMs: 1 },
        ),
      (error: unknown) => isMemoriaError(error, "QUERY_TIMEOUT"),
    );
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
