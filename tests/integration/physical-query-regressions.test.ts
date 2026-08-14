import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

import { createMemoria } from "../../src/engine/create-memoria.js";
import { asSpaceId } from "../../src/domain/ids.js";
import { loadNativeBinding } from "../../src/native/binding.js";
import {
  asQueryStep,
  type NativeQueryResponse,
  type NativeQueryStep,
} from "../../src/native/protocol.js";
import { createBindingHarness } from "../support/remediation.js";

const readinessResponse: NativeQueryResponse = {
  resultCount: 0,
  authorityGeneration: "0",
  degraded: false,
  retrievalId: "RET_readiness",
  results: [],
};

test("readiness pending uses an abortable timer and queryContinue", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-readiness-loop-"));
  const start: NativeQueryStep = {
    state: "readiness-pending",
    operationId: "QO_readiness",
    retryAfterMs: 1,
    deadlineUnixMs: Date.now() + 250,
  };
  const harness = createBindingHarness({
    queryStartResult: start,
    queryContinueResult: { state: "complete", response: readinessResponse },
  });
  const memoria = await createMemoria({
    dataDir,
    binding: harness.binding,
  });

  try {
    const response = await memoria.query({
      scope: { spaces: [asSpaceId("SP_remediation")] },
    });
    assert.equal(response.retrievalId, readinessResponse.retrievalId);
    assert.deepEqual(harness.queryContinueCalls, [start.operationId]);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("text-only public query does not infer semantic capability", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-query-contract-"));
  const harness = createBindingHarness();
  const memoria = await createMemoria({
    dataDir,
    binding: harness.binding,
  });

  try {
    await memoria.query({
      scope: { spaces: [asSpaceId("SP_contract")] },
      cue: { text: "lexical only" },
    });
    assert.equal(harness.queryRequests.length, 1);
    assert.deepEqual(harness.queryRequests[0]?.consistency.required, []);
    assert.deepEqual(harness.queryRequests[0]?.consistency.preferred, []);
  } finally {
    await memoria.close();
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("real native query response exposes the executed physical trace", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-query-trace-"));
  const binding = loadNativeBinding();
  const store = binding.openStore(dataDir);

  try {
    const spaceId = binding.authorityCreateSpace(store, "trace");
    binding.authorityMutate(store, {
      spaceId,
      documentKey: "trace",
      mdx: "# Trace physical lexical cue",
    });
    const step = asQueryStep(
      await binding.queryStart(store, {
        scope: [spaceId],
        cue: { text: "physical lexical cue" },
        history: { mode: "current" },
        consistency: {
          authority: { mode: "latest" },
          required: [],
          preferred: [],
          onNotReady: "fail",
          timeoutMs: 100,
        },
        budget: {
          maxResults: 10,
          maxMatchesPerResult: 3,
          maxEvidenceTokens: 1500,
        },
        quality: "balanced",
        diagnostics: { operatorTrace: true },
      }),
    );

    assert.equal(step.state, "complete");
    if (step.state !== "complete") {
      throw new Error("expected a complete native query response");
    }
    assert(step.response.trace);
    assert(step.response.trace.channelsExecuted.includes("lexical"));
    assert(
      step.response.trace.candidateCounts.some(
        (entry) => entry.channel === "lexical",
      ),
    );
    assert.equal(step.response.trace.rerankApplied, false);
  } finally {
    binding.closeStore(store);
    await rm(dataDir, { recursive: true, force: true });
  }
});

test("required semantic wait returns readiness pending for continuation", async () => {
  const dataDir = await mkdtemp(join(tmpdir(), "memoria-next-readiness-"));
  const binding = loadNativeBinding();
  const store = binding.openStore(dataDir);

  try {
    const spaceId = binding.authorityCreateSpace(store, "readiness");
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
        timeoutMs: 250,
      },
      budget: {
        maxResults: 10,
        maxMatchesPerResult: 3,
        maxEvidenceTokens: 1500,
      },
      quality: "balanced",
    });

    assert("state" in step);
    assert.equal(step.state, "readiness-pending");
    assert.equal(typeof step.retryAfterMs, "number");
    const continued = await binding.queryContinue(store, step.operationId);
    assert("state" in continued);
    assert.notEqual(
      continued.state,
      "complete",
      "required semantic wait must not complete before semantic readiness",
    );
  } finally {
    binding.closeStore(store);
    await rm(dataDir, { recursive: true, force: true });
  }
});
