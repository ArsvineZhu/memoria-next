import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { asSpaceId } from "../../src/domain/ids.js";
import type {
  NativeBinding,
  NativeProviderResult,
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
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-query-operation-"));
  const observed: { result?: ProviderResult; resumed?: NativeProviderResult } = {};
  const store = {
    close() {},
    status() {
      return {
        authorityGeneration: "0",
        baseCoverage: "0",
        semanticCoverage: "0",
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
        type: "pending",
        operationId: "QO_query_operation",
        work: {
          type: "embedding",
          workId: "QW_query_operation",
          signature: "query-embedding-v1",
          dimensions: 3,
          items: [{ key: "query", text: "career" }],
        },
      };
    },
    queryResume(
      _store: unknown,
      _operationId: string,
      result: NativeProviderResult,
    ) {
      observed.resumed = result;
      return { type: "complete", response };
    },
    providerPollWork() {
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
        async execute() {
          return { vectors: [[0, 0, 0]] };
        },
      },
    },
  });

  try {
    const result = await memoria.query({
      scope: { spaces: [asSpaceId("SP_query_operation")] },
      cue: { text: "career" },
      consistency: { preferred: ["semantic"] },
    });
    observed.result = {
      workId: "QW_query_operation",
      accepted: true,
    };
    assert.deepEqual(result, response);
    assert.deepEqual(observed.resumed, observed.result);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});
