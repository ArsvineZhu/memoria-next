import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { asSpaceId } from "../../src/domain/ids.js";
import { loadNativeBinding } from "../../src/native/binding.js";
import type {
  NativeBinding,
  NativeProviderWorkResult,
  NativeQueryRequest,
  NativeQueryResponse,
} from "../../src/native/protocol.js";
import type { ProviderResult } from "../../src/providers/types.js";

const response: NativeQueryResponse = {
  resultCount: 0,
  authorityGeneration: "0",
  degraded: false,
  retrievalId: "RET_query_operation",
  results: [],
};

test("Memoria.query hides start/resume loop from caller", async () => {
  const dataDir = await mkdtemp(
    join(tmpdir(), "memoria-next-query-operation-"),
  );
  const observed: {
    result?: ProviderResult;
    resumed?: NativeProviderWorkResult;
    resumedOperationId?: string;
    providerPollCalls: number;
  } = { providerPollCalls: 0 };
  const store = {
    close() {},
    status() {
      return {
        authorityGeneration: "0",
        baseCoverage: "0",
        semanticCoverage: "0",
        semanticBuildCoverage: "0",
        activeReadLeases: 0,
        closed: false,
      };
    },
  };
  const binding = {
    openStore() {
      return store;
    },
    closeStore() {},
    authorityCreateSpace() {
      return "SP_query_operation";
    },
    authorityMutate() {
      return "M_query_operation";
    },
    authorityRevise() {
      return {
        memoryId: "M_query_operation",
        spaceId: "SP_query_operation",
        revisionId: "R_query_operation",
        authorityGeneration: "1",
      };
    },
    exportMemories() {
      return [];
    },
    purgePlan() {
      return {
        id: "PURGE_query_operation",
        memoryId: "M_query_operation",
        state: "planned" as const,
      };
    },
    purgeExecute() {
      return {
        id: "PURGE_query_operation",
        memoryId: "M_query_operation",
        state: "completed" as const,
      };
    },
    queryStart(_store: unknown, _request: NativeQueryRequest) {
      return {
        state: "pending",
        operationId: "QO_query_operation",
        work: {
          type: "query-embedding",
          workId: "QW_query_operation",
          signature: "query-embedding-v1",
          input: { key: "query", text: "career" },
        },
      };
    },
    queryResume(
      _store: unknown,
      operationId: string,
      result: NativeProviderWorkResult,
    ) {
      observed.resumedOperationId = operationId;
      observed.resumed = result;
      return { state: "complete", response };
    },
    queryContinue() {
      return { state: "complete", response };
    },
    providerPollWork() {
      observed.providerPollCalls += 1;
      return null;
    },
    providerSubmitResult() {},
    feedbackSubmit() {
      return { generation: "0", events: [] };
    },
    readSessionOpen() {
      return "RS_query_operation";
    },
    readSessionClose() {},
    cancelOperation() {},
  } as unknown as NativeBinding;
  const memoria = await createMemoria({
    dataDir,
    binding,
    providers: {
      embedding: {
        trust: "external",
        async execute() {
          return { vectors: [{ key: "query", values: [0, 0, 0] }] };
        },
      },
    },
  });
  observed.providerPollCalls = 0;

  try {
    const result = await memoria.query({
      scope: { spaces: [asSpaceId("SP_query_operation")] },
      cue: { text: "career" },
      consistency: { preferred: ["semantic"] },
    });
    observed.result = {
      type: "embeddings",
      workId: "QW_query_operation",
      vectors: [{ key: "query", values: [0, 0, 0] }],
    };
    assert.deepEqual(result, response);
    assert.equal(observed.resumedOperationId, "QO_query_operation");
    assert.equal(observed.providerPollCalls, 0);
    assert.deepEqual(observed.resumed, observed.result);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("real native query operation exposes readiness continuation and cancellation", async () => {
  const dataDir = await mkdtemp(
    join(tmpdir(), "memoria-next-native-query-op-"),
  );
  const binding = loadNativeBinding();
  const store = binding.openStore(dataDir);

  try {
    const spaceId = binding.authorityCreateSpace(
      store,
      "query-operation-native",
    );
    binding.authorityMutate(store, {
      spaceId,
      documentKey: "career",
      mdx: "# Career\nRust systems work",
    });
    const step = await binding.queryStart(store, {
      scope: [spaceId],
      cue: { text: "career" },
      history: { mode: "current" },
      consistency: {
        authority: { mode: "latest" },
        required: ["semantic"],
        preferred: [],
        onNotReady: "wait",
        timeoutMs: 1000,
      },
      budget: {
        maxResults: 10,
        maxMatchesPerResult: 3,
        maxEvidenceTokens: 1500,
      },
      quality: "balanced",
    });

    assert("state" in step);
    if (step.state !== "readiness-pending") {
      throw new Error("expected native query operation to be readiness pending");
    }
    binding.cancelOperation(store, step.operationId);
    assert.throws(
      () => binding.queryContinue(store, step.operationId),
      /ABORTED/,
    );
  } finally {
    binding.closeStore(store);
    await rm(dataDir, { recursive: true, force: true });
  }
});
