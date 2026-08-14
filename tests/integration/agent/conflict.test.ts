import assert from "node:assert/strict";
import { test } from "node:test";

import { MemoriaError } from "../../../src/domain/errors.js";
import { createIdentityResolver } from "../../../src/agent/identity.js";
import {
  createAgentTools,
  type AgentBackend,
} from "../../../src/agent/tools.js";

test("agent update does not blind-retry after head conflict", async () => {
  let currentHead = "R_initial";
  let updateCalls = 0;
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
        revisionId: currentHead,
        mdx: "# latest\n",
        entityRefs: ["person:USER"],
      };
    },
    async createMemory() {
      throw new Error("not used");
    },
    async updateMemory() {
      updateCalls += 1;
      throw new MemoriaError(
        "HEAD_CONFLICT",
        "HEAD_CONFLICT: expected revision is stale",
      );
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
    permissions: { read: true, write: true },
  });

  const stale = await tools.readMemory({ id: "M_shared" });
  currentHead = "R_other-writer";
  const result = await tools.updateMemory({
    memoryId: "M_shared",
    expectedHead: stale.revisionId,
    operations: [
      { type: "replace", target: "state:current", value: "new value" },
    ],
  });

  assert.equal(result.status, "conflict");
  if (result.status === "conflict") {
    assert.equal(result.actualHead, currentHead);
    assert.equal(result.autoRetried, false);
    assert.equal(result.recovery, "read-latest-head-and-regenerate");
  }
  assert.equal(updateCalls, 1);
});
