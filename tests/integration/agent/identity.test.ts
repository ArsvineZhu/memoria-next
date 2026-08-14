import assert from "node:assert/strict";
import { test } from "node:test";

import { createIdentityResolver } from "../../../src/agent/identity.js";

test("ambiguous observed alias is not auto-bound", async () => {
  const resolver = createIdentityResolver({
    conversationBindings: [],
    directory: {
      async find() {
        return [];
      },
    },
    observations: {
      async discover() {
        return [
          { entityRef: "person:A", surface: "小王", spaceId: "personal" },
          { entityRef: "person:B", surface: "小王", spaceId: "personal" },
        ];
      },
    },
  });

  const result = await resolver.resolve({
    surface: "小王",
    spaces: ["personal"],
  });

  assert.equal(result.status, "ambiguous");
  if (result.status === "ambiguous") {
    assert.deepEqual(
      result.candidates.map((candidate) => candidate.entityRef),
      ["person:A", "person:B"],
    );
  }
});

test("conversation binding wins over directory and observations", async () => {
  const calls: string[] = [];
  const resolver = createIdentityResolver({
    conversationBindings: [
      { surface: "小王", entityRef: "person:conversation" },
    ],
    directory: {
      async find() {
        calls.push("directory");
        return [{ entityRef: "person:directory" }];
      },
    },
    observations: {
      async discover() {
        calls.push("observations");
        return [
          {
            entityRef: "person:observed",
            surface: "小王",
            spaceId: "personal",
          },
        ];
      },
    },
  });

  const result = await resolver.resolve({
    surface: "小王",
    spaces: ["personal"],
  });

  assert.equal(result.status, "resolved");
  if (result.status === "resolved") {
    assert.equal(result.entityRef, "person:conversation");
    assert.equal(result.source, "conversation");
  }
  assert.deepEqual(calls, []);
});

test("one observed candidate still requires confirmation", async () => {
  const resolver = createIdentityResolver({
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
            entityRef: "person:observed",
            surface: "小王",
            spaceId: "personal",
          },
        ];
      },
    },
  });

  const result = await resolver.resolve({
    surface: "小王",
    spaces: ["personal"],
  });

  assert.equal(result.status, "ambiguous");
  if (result.status === "ambiguous") {
    assert.equal(result.reason, "observation-requires-confirmation");
    assert.equal(result.candidates[0]?.source, "observation");
  }
});

test("identity resolution reports a scoped miss", async () => {
  const resolver = createIdentityResolver({
    conversationBindings: [],
    directory: {
      async find() {
        return [];
      },
    },
    observations: {
      async discover(input) {
        assert.deepEqual(input, {
          surface: "小王",
          spaces: ["personal"],
        });
        return [
          { entityRef: "person:other", surface: "小李", spaceId: "personal" },
          { entityRef: "person:outside", surface: "小王", spaceId: "work" },
        ];
      },
    },
  });

  const result = await resolver.resolve({
    surface: "小王",
    spaces: ["personal"],
  });

  assert.deepEqual(result, { status: "not-found", candidates: [] });
});
