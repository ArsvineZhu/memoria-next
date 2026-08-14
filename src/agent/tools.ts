import { MemoriaError, toMemoriaError } from "../domain/errors.js";
import type {
  IdentityResolution,
  IdentityResolutionRequest,
  IdentityResolver,
} from "./identity.js";
import {
  discoverMemoryOwner,
  type AgentMemory,
  type AgentQueryHit,
  type AgentSpace,
  type DiscoverMemoryOwnerInput,
  type MemoryOwnerResult,
  type OwnerDiscoveryBackend,
} from "./memory-owner.js";

export type AgentPermission =
  "discover" | "read" | "write" | "feedback" | "lifecycle";

export type AgentPermissionPolicy = Partial<Record<AgentPermission, boolean>>;

export type MemoryPatchOperation =
  | {
      readonly type: "insert";
      readonly target: string;
      readonly value: string;
    }
  | {
      readonly type: "replace";
      readonly target: string;
      readonly value: string;
    }
  | {
      readonly type: "remove";
      readonly target: string;
    }
  | {
      readonly type: "set-state";
      readonly target: string;
      readonly value: string;
    }
  | {
      readonly type: "set-time";
      readonly target: string;
      readonly value: string;
    };

export interface AgentFeedbackInput {
  readonly retrievalId: string;
  readonly idempotencyKey: string;
  readonly events: readonly {
    readonly resultId: string;
    readonly outcome: string;
  }[];
}

export interface AgentFeedbackResult {
  readonly generation: string;
  readonly events: readonly Record<string, unknown>[];
}

export interface AgentBackend extends OwnerDiscoveryBackend {
  createMemory(input: {
    spaceId: string;
    documentKey?: string;
    idempotencyKey?: string;
    mdx: string;
    entityRefs: readonly string[];
  }): Promise<AgentMemory>;
  updateMemory(input: {
    memoryId: string;
    expectedHead: string;
    operations: readonly MemoryPatchOperation[];
  }): Promise<AgentMemory>;
  transitionMemoryState(input: {
    memoryId: string;
    expectedHead: string;
    operations: readonly MemoryPatchOperation[];
  }): Promise<AgentMemory>;
  correctMemory(input: {
    memoryId: string;
    expectedHead: string;
    operations: readonly MemoryPatchOperation[];
  }): Promise<AgentMemory>;
  submitFeedback(input: AgentFeedbackInput): Promise<AgentFeedbackResult>;
  forgetMemory(input: {
    memoryId: string;
    mode: "patch" | "retire" | "purge";
    expectedHead?: string;
    operations?: readonly MemoryPatchOperation[];
  }): Promise<{ status: string; memoryId: string }>;
}

export interface AgentMutationSuccess {
  readonly status: "updated" | "transitioned" | "corrected";
  readonly memory: AgentMemory;
}

export interface AgentConflict {
  readonly status: "conflict";
  readonly memoryId: string;
  readonly actualHead: string | null;
  readonly autoRetried: false;
  readonly recovery:
    "read-latest-head-and-regenerate" | "read-latest-head-and-ask";
}

export type AgentMutationResult = AgentMutationSuccess | AgentConflict;

export interface AgentTools {
  listMemorySpaces(): Promise<readonly AgentSpace[]>;
  findEntity(input: IdentityResolutionRequest): Promise<IdentityResolution>;
  findMemory(input: DiscoverMemoryOwnerInput): Promise<MemoryOwnerResult>;
  discoverMemoryOwner(
    input: DiscoverMemoryOwnerInput,
  ): Promise<MemoryOwnerResult>;
  readMemory(input: { id: string; revisionId?: string }): Promise<AgentMemory>;
  recordMemory(input: {
    space: { key: string };
    mdx: string;
    entityRefs: readonly string[];
    documentKey?: string;
    idempotencyKey?: string;
  }): Promise<AgentMemory>;
  updateMemory(input: {
    memoryId: string;
    expectedHead: string;
    operations: readonly MemoryPatchOperation[];
  }): Promise<AgentMutationResult>;
  transitionMemoryState(input: {
    memoryId: string;
    expectedHead: string;
    operations: readonly MemoryPatchOperation[];
  }): Promise<AgentMutationResult>;
  correctMemory(input: {
    memoryId: string;
    expectedHead: string;
    operations: readonly MemoryPatchOperation[];
  }): Promise<AgentMutationResult>;
  submitMemoryFeedback(input: AgentFeedbackInput): Promise<AgentFeedbackResult>;
  forgetMemory(input: {
    memoryId: string;
    mode: "patch" | "retire" | "purge";
    expectedHead?: string;
    operations?: readonly MemoryPatchOperation[];
  }): Promise<{ status: string; memoryId: string }>;
}

export interface AgentToolsOptions {
  readonly backend: AgentBackend;
  readonly identity: IdentityResolver;
  readonly permissions?: AgentPermissionPolicy;
}

const DEFAULT_PERMISSIONS: Required<AgentPermissionPolicy> = {
  discover: false,
  read: false,
  write: false,
  feedback: false,
  lifecycle: false,
};

export function createAgentTools(options: AgentToolsOptions): AgentTools {
  const permissions = {
    ...DEFAULT_PERMISSIONS,
    ...options.permissions,
  };
  const requirePermission = (permission: AgentPermission): void => {
    if (!permissions[permission]) {
      throw new MemoriaError(
        "UNSUPPORTED_OPERATION",
        "AGENT_PERMISSION_DENIED: " + permission,
      );
    }
  };
  const resolveSpace = async (key: string): Promise<AgentSpace> => {
    const spaces = await options.backend.listSpaces();
    const space = spaces.find((candidate) => candidate.key === key);
    if (!space) {
      throw new MemoriaError("NOT_FOUND", "NOT_FOUND: memory Space " + key);
    }
    return space;
  };
  const mutate = async (
    memoryId: string,
    action: () => Promise<AgentMemory>,
    status: AgentMutationSuccess["status"],
  ): Promise<AgentMutationResult> => {
    try {
      return { status, memory: await action() };
    } catch (error) {
      const memoriaError = toMemoriaError(error);
      if (memoriaError.code !== "HEAD_CONFLICT") {
        throw memoriaError;
      }
      let actualHead: string | null = null;
      try {
        actualHead = (await options.backend.readMemory({ memoryId }))
          .revisionId;
      } catch {
        // The conflict remains recoverable even if the follow-up read is unavailable.
      }
      return {
        status: "conflict",
        memoryId,
        actualHead,
        autoRetried: false,
        recovery:
          actualHead === null
            ? "read-latest-head-and-ask"
            : "read-latest-head-and-regenerate",
      };
    }
  };

  return {
    async listMemorySpaces() {
      requirePermission("discover");
      return options.backend.listSpaces();
    },
    async findEntity(input) {
      requirePermission("discover");
      return options.identity.resolve(input);
    },
    async findMemory(input) {
      requirePermission("discover");
      requirePermission("read");
      return discoverMemoryOwner(options.backend, input);
    },
    async discoverMemoryOwner(input) {
      requirePermission("discover");
      requirePermission("read");
      return discoverMemoryOwner(options.backend, input);
    },
    async readMemory(input) {
      requirePermission("read");
      return options.backend.readMemory({
        memoryId: input.id,
        ...(input.revisionId === undefined
          ? {}
          : { revisionId: input.revisionId }),
      });
    },
    async recordMemory(input) {
      requirePermission("write");
      const space = await resolveSpace(input.space.key);
      return options.backend.createMemory({
        spaceId: space.id,
        ...(input.documentKey === undefined
          ? {}
          : { documentKey: input.documentKey }),
        ...(input.idempotencyKey === undefined
          ? {}
          : { idempotencyKey: input.idempotencyKey }),
        mdx: input.mdx,
        entityRefs: [...input.entityRefs],
      });
    },
    async updateMemory(input) {
      requirePermission("write");
      return mutate(
        input.memoryId,
        () => options.backend.updateMemory(input),
        "updated",
      );
    },
    async transitionMemoryState(input) {
      requirePermission("write");
      return mutate(
        input.memoryId,
        () => options.backend.transitionMemoryState(input),
        "transitioned",
      );
    },
    async correctMemory(input) {
      requirePermission("write");
      return mutate(
        input.memoryId,
        () => options.backend.correctMemory(input),
        "corrected",
      );
    },
    async submitMemoryFeedback(input) {
      requirePermission("feedback");
      return options.backend.submitFeedback(input);
    },
    async forgetMemory(input) {
      requirePermission("lifecycle");
      return options.backend.forgetMemory(input);
    },
  };
}

export type { AgentQueryHit };
