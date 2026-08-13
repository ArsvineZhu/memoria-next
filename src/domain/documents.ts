import {
  asMemoryId,
  asRevisionId,
  asSpaceId,
  type MemoryId,
  type RevisionId,
} from "./ids.js";
import { toMemoriaError } from "./errors.js";
import type { Space } from "./spaces.js";

export interface CreateDocumentInput {
  space: Pick<Space, "id">;
  documentKey?: string;
  mdx: string;
  idempotencyKey?: string;
}

export interface CreatedDocument {
  memoryId: MemoryId;
  spaceId: ReturnType<typeof asSpaceId>;
  revisionId?: RevisionId;
  authorityGeneration: string;
}

export interface ReviseDocumentInput {
  memory: { id: MemoryId };
  expectedHead: RevisionId;
  mdx: string;
}

export interface RevisedDocument {
  memoryId: MemoryId;
  spaceId: ReturnType<typeof asSpaceId>;
  revisionId: RevisionId;
  authorityGeneration: string;
}

interface DocumentEngine {
  createMemory(input: {
    spaceId: string;
    documentKey?: string;
    idempotencyKey?: string;
    mdx: string;
  }): Promise<{ memoryId: string; authorityGeneration: string }>;
  reviseMemory(input: {
    memoryId: string;
    expectedHead: string;
    mdx: string;
  }): Promise<{
    memoryId: string;
    spaceId: string;
    revisionId: string;
    authorityGeneration: string;
  }>;
}

export interface DocumentsApi {
  create(input: CreateDocumentInput): Promise<CreatedDocument>;
  revise(input: ReviseDocumentInput): Promise<RevisedDocument>;
}

export function createDocumentsApi(engine: DocumentEngine): DocumentsApi {
  return {
    async create(input) {
      const created = await engine.createMemory({
        spaceId: input.space.id,
        ...(input.documentKey === undefined
          ? {}
          : { documentKey: input.documentKey }),
        ...(input.idempotencyKey === undefined
          ? {}
          : { idempotencyKey: input.idempotencyKey }),
        mdx: input.mdx,
      });
      return {
        memoryId: asMemoryId(created.memoryId),
        spaceId: input.space.id,
        authorityGeneration: created.authorityGeneration,
      };
    },
    async revise(input) {
      try {
        const revised = await engine.reviseMemory({
          memoryId: input.memory.id,
          expectedHead: input.expectedHead,
          mdx: input.mdx,
        });
        return {
          memoryId: asMemoryId(revised.memoryId),
          spaceId: asSpaceId(revised.spaceId),
          revisionId: asRevisionId(revised.revisionId),
          authorityGeneration: revised.authorityGeneration,
        };
      } catch (error) {
        throw toMemoriaError(error);
      }
    },
  };
}
