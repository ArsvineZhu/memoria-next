import type { SpaceId } from "./ids.js";

export interface MemoryQueryInput {
  scope: Array<SpaceId | { id: SpaceId }>;
  text?: string;
}

export interface NormalizedMemoryQuery {
  scope: string[];
  text?: string;
}

export function normalizeQuery(input: MemoryQueryInput): NormalizedMemoryQuery {
  return {
    scope: input.scope.map((space) =>
      typeof space === "string" ? space : space.id,
    ),
    ...(input.text === undefined ? {} : { text: input.text }),
  };
}
