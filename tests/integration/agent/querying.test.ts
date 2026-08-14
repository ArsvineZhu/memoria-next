import assert from "node:assert/strict";
import { test } from "node:test";

import { createIdentityResolver } from "../../../src/agent/identity.js";
import {
  createAgentTools,
  type AgentBackend,
} from "../../../src/agent/tools.js";

test("querying an authorized miss does not create a memory", async () => {
  let created = 0;
  const backend: AgentBackend = {
    async listSpaces() {
      return [{ id: "SP_personal", key: "personal" }];
    },
    async query(input) {
      assert.deepEqual(input, {
        scope: ["SP_personal"],
        cue: "historical education",
      });
      return [];
    },
    async readMemory() {
      throw new Error("read should not run for an empty query");
    },
    async createMemory() {
      created += 1;
      throw new Error("query miss must not create");
    },
    async updateMemory() {
      throw new Error("not used");
    },
    async transitionMemoryState() {
      throw new Error("not used");
    },
    async correctMemory() {
      throw new Error("not used");
    },
    async submitFeedback() {
      throw new Error("not used");
    },
    async forgetMemory() {
      throw new Error("not used");
    },
  };
  const tools = createAgentTools({
    backend,
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
    permissions: { discover: true, read: true },
  });

  const result = await tools.findMemory({
    spaces: [{ key: "personal" }],
    cue: "historical education",
    entities: [],
  });

  assert.equal(result.kind, "none-found");
  assert.equal(created, 0);
});

test("read_memory returns authoritative content only through the narrow tool", async () => {
  const backend: AgentBackend = {
    async listSpaces() {
      return [];
    },
    async query() {
      return [];
    },
    async readMemory(input) {
      return {
        memoryId: input.memoryId,
        spaceId: "SP_personal",
        revisionId: "R_current",
        mdx: "# Current memory\n",
        entityRefs: ["person:USER"],
      };
    },
    async createMemory() {
      throw new Error("not used");
    },
    async updateMemory() {
      throw new Error("not used");
    },
    async transitionMemoryState() {
      throw new Error("not used");
    },
    async correctMemory() {
      throw new Error("not used");
    },
    async submitFeedback() {
      throw new Error("not used");
    },
    async forgetMemory() {
      throw new Error("not used");
    },
  };
  const tools = createAgentTools({
    backend,
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
    permissions: { read: true },
  });

  const memory = await tools.readMemory({ id: "M_current" });
  assert.equal(memory.revisionId, "R_current");
  assert.equal(memory.mdx, "# Current memory\n");
});
