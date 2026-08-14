import assert from "node:assert/strict";
import { test } from "node:test";

import { createIdentityResolver } from "../../../src/agent/identity.js";
import {
  createAgentTools,
  type AgentBackend,
} from "../../../src/agent/tools.js";
import {
  ProviderHost,
  createProviderEgressGuard,
} from "../../../src/providers/host.js";

function governanceBackend(calls: string[]): AgentBackend {
  const memory = {
    memoryId: "M_private",
    spaceId: "SP_private",
    revisionId: "R_private",
    mdx: "# private\n",
    entityRefs: ["person:USER"],
  };
  return {
    async listSpaces() {
      return [{ id: "SP_private", key: "private" }];
    },
    async query() {
      return [];
    },
    async readMemory() {
      return memory;
    },
    async createMemory() {
      return memory;
    },
    async updateMemory() {
      return memory;
    },
    async transitionMemoryState() {
      return memory;
    },
    async correctMemory() {
      return memory;
    },
    async submitFeedback() {
      return { generation: "1", events: [] };
    },
    async forgetMemory(input) {
      calls.push(input.mode + ":" + input.memoryId);
      return { status: input.mode, memoryId: input.memoryId };
    },
  };
}

test("forget routes to the lifecycle governance backend", async () => {
  const calls: string[] = [];
  const tools = createAgentTools({
    backend: governanceBackend(calls),
    identity: createIdentityResolver({
      conversationBindings: [],
      directory: {
        async find() {
          return [];
        },
      },
      observations: {
        async discover() {
          return [];
        },
      },
    }),
    permissions: { lifecycle: true },
  });

  const result = await tools.forgetMemory({
    memoryId: "M_private",
    mode: "purge",
  });

  assert.deepEqual(result, { status: "purge", memoryId: "M_private" });
  assert.deepEqual(calls, ["purge:M_private"]);
});

test("local-only Space blocks provider egress before provider execution", async () => {
  let calls = 0;
  const host = new ProviderHost({
    providers: {
      embedding: {
        trust: "external",
        async execute() {
          calls += 1;
          return { vectors: [{ key: "memory-1", values: [1, 0] }] };
        },
      },
    },
    onDataEgress: createProviderEgressGuard({
      embedding: false,
      rerank: false,
      enrichment: false,
    }),
  });

  await assert.rejects(
    () =>
      host.execute(
        {
          type: "embedding",
          workId: "WORK_private",
          signature: "local-only",
          dimensions: 2,
          items: [{ key: "M_private", text: "private source" }],
        },
        new AbortController().signal,
      ),
    (error: unknown) =>
      error instanceof Error && error.message.includes("CAPABILITY_NOT_READY"),
  );
  assert.equal(calls, 0);
});
