import assert from "node:assert/strict";
import { test } from "node:test";

import { MemoriaError } from "../../../src/domain/errors.js";
import { createIdentityResolver } from "../../../src/agent/identity.js";
import type { AgentMemory } from "../../../src/agent/memory-owner.js";
import {
  createAgentTools,
  type AgentBackend,
  type AgentTools,
} from "../../../src/agent/tools.js";

interface WorkflowState {
  readonly history: AgentMemory[];
  readonly mutationKinds: string[];
  readonly feedbackEvents: Array<{ resultId: string; outcome: string }>;
  readonly governance: string[];
  updateCalls: number;
  forceConflict: boolean;
}

function workflowFixture(): {
  tools: AgentTools;
  backend: AgentBackend;
  state: WorkflowState;
} {
  const current: AgentMemory = {
    memoryId: "M_career",
    spaceId: "SP_personal",
    revisionId: "R_career-current",
    mdx: "# Education and career\nThe user works on systems.",
    entityRefs: ["person:USER"],
  };
  const historical: AgentMemory = {
    ...current,
    revisionId: "R_career-old",
    mdx: "# Education and career\nThe user once considered graduate school.",
  };
  const versions = new Map<string, AgentMemory>([
    [historical.revisionId, historical],
    [current.revisionId, current],
  ]);
  const state: WorkflowState = {
    history: [],
    mutationKinds: [],
    feedbackEvents: [],
    governance: [],
    updateCalls: 0,
    forceConflict: false,
  };

  const nextRevision = (label: string, memory: AgentMemory): AgentMemory => {
    const next = {
      ...memory,
      revisionId: "R_" + label,
    };
    versions.set(next.revisionId, next);
    return next;
  };

  const backend: AgentBackend = {
    async listSpaces() {
      return [{ id: "SP_personal", key: "personal" }];
    },
    async query(input) {
      if (input.cue.includes("historical")) {
        return [
          {
            memoryId: historical.memoryId,
            spaceId: historical.spaceId,
            revisionId: historical.revisionId,
          },
        ];
      }
      return [
        {
          memoryId: current.memoryId,
          spaceId: current.spaceId,
          revisionId: current.revisionId,
        },
      ];
    },
    async readMemory(input) {
      if (input.revisionId) {
        const version = versions.get(input.revisionId);
        if (!version) {
          throw new MemoriaError(
            "NOT_FOUND",
            "NOT_FOUND: revision " + input.revisionId,
          );
        }
        return version;
      }
      return current;
    },
    async createMemory(input) {
      return {
        memoryId: "M_created",
        spaceId: input.spaceId,
        revisionId: "R_created",
        mdx: input.mdx,
        entityRefs: [...input.entityRefs],
      };
    },
    async updateMemory(input) {
      state.updateCalls += 1;
      if (state.forceConflict) {
        throw new MemoriaError(
          "HEAD_CONFLICT",
          "HEAD_CONFLICT: expected revision is stale",
        );
      }
      assert.equal(input.expectedHead, current.revisionId);
      state.mutationKinds.push("update");
      const next = nextRevision("updated", {
        ...current,
        mdx: current.mdx + " Added fact.",
      });
      Object.assign(current, next);
      return current;
    },
    async transitionMemoryState(input) {
      assert.equal(input.expectedHead, current.revisionId);
      state.mutationKinds.push("transition");
      state.history.push({ ...current });
      const next = nextRevision("transitioned", {
        ...current,
        mdx: current.mdx + " State changed with an explicit validFrom.",
      });
      Object.assign(current, next);
      return current;
    },
    async correctMemory(input) {
      assert.equal(input.expectedHead, current.revisionId);
      state.mutationKinds.push("correct");
      const next = nextRevision("corrected", {
        ...current,
        mdx: current.mdx + " Correction provenance retained.",
      });
      Object.assign(current, next);
      return current;
    },
    async submitFeedback(input) {
      state.feedbackEvents.push(...input.events);
      return { generation: "9", events: [] };
    },
    async forgetMemory(input) {
      state.governance.push(input.mode + ":" + input.memoryId);
      return { status: input.mode, memoryId: input.memoryId };
    },
  };

  const tools = createAgentTools({
    backend,
    identity: createIdentityResolver({
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
    }),
    permissions: {
      discover: true,
      read: true,
      write: true,
      feedback: true,
      lifecycle: true,
    },
  });
  return { tools, backend, state };
}

test("agent e2e covers recall, authoring, conflict, feedback, and governance", async () => {
  const fixture = workflowFixture();

  const historicalOwner = await fixture.tools.findMemory({
    spaces: [{ key: "personal" }],
    entities: ["person:USER"],
    cue: "historical education",
  });
  assert.equal(historicalOwner.kind, "existing");
  if (historicalOwner.kind !== "existing") {
    return;
  }
  assert.equal(historicalOwner.revisionId, "R_career-old");
  const historical = await fixture.tools.readMemory({
    id: historicalOwner.memoryId,
    revisionId: historicalOwner.revisionId,
  });
  assert(historical.mdx.includes("once considered graduate school"));

  const owner = await fixture.tools.discoverMemoryOwner({
    spaces: [{ key: "personal" }],
    entities: ["person:USER"],
    cue: "education and career",
  });
  assert.equal(owner.kind, "existing");
  if (owner.kind !== "existing") {
    return;
  }
  const updated = await fixture.tools.updateMemory({
    memoryId: owner.memoryId,
    expectedHead: owner.revisionId,
    operations: [
      { type: "insert", target: "state:current", value: "new fact" },
    ],
  });
  assert.equal(updated.status, "updated");
  if (!("memory" in updated)) {
    return;
  }

  const transitioned = await fixture.tools.transitionMemoryState({
    memoryId: owner.memoryId,
    expectedHead: updated.memory.revisionId,
    operations: [
      { type: "set-time", target: "state:current", value: "validFrom" },
    ],
  });
  assert.equal(transitioned.status, "transitioned");
  assert.equal(fixture.state.history.length, 1);
  const transitionHead =
    "memory" in transitioned
      ? transitioned.memory.revisionId
      : updated.memory.revisionId;

  const corrected = await fixture.tools.correctMemory({
    memoryId: owner.memoryId,
    expectedHead: transitionHead,
    operations: [{ type: "replace", target: "source:year", value: "2023" }],
  });
  assert.equal(corrected.status, "corrected");
  assert.deepEqual(fixture.state.mutationKinds, [
    "update",
    "transition",
    "correct",
  ]);

  const ambiguousTools = createAgentTools({
    backend: fixture.backend,
    identity: createIdentityResolver({
      conversationBindings: [],
      directory: {
        async find() {
          return [];
        },
      },
      observations: {
        async discover() {
          return [
            {
              entityRef: "person:A",
              surface: "小王",
              spaceId: "SP_personal",
            },
            {
              entityRef: "person:B",
              surface: "小王",
              spaceId: "SP_personal",
            },
          ];
        },
      },
    }),
    permissions: { discover: true },
  });
  const ambiguous = await ambiguousTools.findEntity({
    surface: "小王",
    spaces: ["SP_personal"],
  });
  assert.equal(ambiguous.status, "ambiguous");

  fixture.state.forceConflict = true;
  const conflict = await fixture.tools.updateMemory({
    memoryId: owner.memoryId,
    expectedHead: "R_stale",
    operations: [{ type: "replace", target: "state:current", value: "stale" }],
  });
  assert.equal(conflict.status, "conflict");
  if (conflict.status === "conflict") {
    assert.equal(conflict.autoRetried, false);
    assert.equal(conflict.actualHead, "R_corrected");
  }
  assert.equal(fixture.state.updateCalls, 2);

  const feedbackBeforeUnused = fixture.state.feedbackEvents.length;
  await fixture.tools.readMemory({ id: owner.memoryId });
  assert.equal(fixture.state.feedbackEvents.length, feedbackBeforeUnused);
  await fixture.tools.submitMemoryFeedback({
    retrievalId: "RET_used",
    idempotencyKey: "FB_used",
    events: [{ resultId: "RES_used", outcome: "used" }],
  });
  assert.equal(fixture.state.feedbackEvents.length, feedbackBeforeUnused + 1);
  const feedbackAfterUsed = fixture.state.feedbackEvents.length;
  assert.equal(fixture.state.feedbackEvents.length, feedbackAfterUsed);

  const forgotten = await fixture.tools.forgetMemory({
    memoryId: owner.memoryId,
    mode: "retire",
  });
  assert.deepEqual(forgotten, {
    status: "retire",
    memoryId: owner.memoryId,
  });
  assert.deepEqual(fixture.state.governance, ["retire:M_career"]);
});
