import assert from "node:assert/strict";
import { test } from "node:test";

import { createIdentityResolver } from "../../../src/agent/identity.js";
import {
  createAgentTools,
  type AgentBackend,
} from "../../../src/agent/tools.js";

function careerBackend(): AgentBackend {
  const career = {
    memoryId: "M_career",
    spaceId: "SP_personal",
    revisionId: "R_career-1",
    mdx: "# Education and career\n",
    entityRefs: ["person:USER"],
  };
  return {
    async listSpaces() {
      return [{ id: "SP_personal", key: "personal" }];
    },
    async query() {
      return [
        {
          memoryId: career.memoryId,
          spaceId: career.spaceId,
          revisionId: career.revisionId,
        },
      ];
    },
    async readMemory() {
      return career;
    },
    async createMemory(input) {
      return {
        memoryId: "M_new",
        spaceId: input.spaceId,
        revisionId: "R_new",
        mdx: input.mdx,
        entityRefs: input.entityRefs,
      };
    },
    async updateMemory() {
      return career;
    },
    async transitionMemoryState() {
      return career;
    },
    async correctMemory() {
      return career;
    },
    async submitFeedback() {
      return { generation: "2", events: [] };
    },
    async forgetMemory() {
      return { status: "retired", memoryId: career.memoryId };
    },
  };
}

function agentFixture() {
  const identity = createIdentityResolver({
    conversationBindings: [{ surface: "我", entityRef: "person:USER" }],
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
  });
  return createAgentTools({
    backend: careerBackend(),
    identity,
    permissions: {
      discover: true,
      read: true,
      write: true,
      feedback: true,
      lifecycle: true,
    },
  });
}

test("long-term career information selects existing owner", async () => {
  const tools = agentFixture();
  const result = await tools.discoverMemoryOwner({
    spaces: [{ key: "personal" }],
    entities: ["person:USER"],
    cue: "用户的教育与职业方向",
  });

  assert.equal(result.kind, "existing");
  if (result.kind === "existing") {
    assert.equal(result.memoryId, "M_career");
  }
});

test("recording is explicit after owner discovery", async () => {
  const tools = agentFixture();
  const result = await tools.recordMemory({
    space: { key: "personal" },
    mdx: "# A durable fact\n",
    entityRefs: ["person:USER"],
    idempotencyKey: "agent-record-1",
  });

  assert.equal(result.memoryId, "M_new");
  assert.equal(result.spaceId, "SP_personal");
});

test("update, transition, correction, and feedback remain separate tools", async () => {
  const tools = agentFixture();
  const operations = [
    { type: "insert" as const, target: "state:current", value: "Rust" },
  ];

  const updated = await tools.updateMemory({
    memoryId: "M_career",
    expectedHead: "R_career-1",
    operations,
  });
  const transitioned = await tools.transitionMemoryState({
    memoryId: "M_career",
    expectedHead: "R_career-1",
    operations,
  });
  const corrected = await tools.correctMemory({
    memoryId: "M_career",
    expectedHead: "R_career-1",
    operations,
  });
  const feedback = await tools.submitMemoryFeedback({
    retrievalId: "RET_1",
    idempotencyKey: "feedback-1",
    events: [{ resultId: "RES_1", outcome: "used" }],
  });

  assert.equal(updated.status, "updated");
  assert.equal(transitioned.status, "transitioned");
  assert.equal(corrected.status, "corrected");
  assert.equal(feedback.generation, "2");
});
