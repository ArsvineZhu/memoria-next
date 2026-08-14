export interface AgentSpace {
  readonly id: string;
  readonly key: string;
}

export interface AgentMemory {
  readonly memoryId: string;
  readonly spaceId: string;
  readonly revisionId: string;
  readonly mdx: string;
  readonly entityRefs: readonly string[];
  readonly documentKey?: string;
}

export interface AgentQueryHit {
  readonly memoryId: string;
  readonly spaceId: string;
  readonly revisionId: string;
}

export interface OwnerDiscoveryBackend {
  listSpaces(): Promise<readonly AgentSpace[]>;
  query(input: {
    scope: readonly string[];
    cue: string;
  }): Promise<readonly AgentQueryHit[]>;
  readMemory(input: { memoryId: string }): Promise<AgentMemory>;
}

export interface DiscoverMemoryOwnerInput {
  readonly spaces: readonly { key: string }[];
  readonly entities: readonly string[];
  readonly cue: string;
}

export interface MemoryOwnerCandidate {
  readonly memoryId: string;
  readonly spaceId: string;
  readonly revisionId: string;
  readonly documentKey?: string;
}

export type MemoryOwnerResult =
  | {
      readonly kind: "existing";
      readonly memoryId: string;
      readonly spaceId: string;
      readonly revisionId: string;
      readonly documentKey?: string;
    }
  | {
      readonly kind: "ambiguous";
      readonly candidates: readonly MemoryOwnerCandidate[];
    }
  | {
      readonly kind: "none-found";
      readonly scope: readonly string[];
    };

export async function discoverMemoryOwner(
  backend: OwnerDiscoveryBackend,
  input: DiscoverMemoryOwnerInput,
): Promise<MemoryOwnerResult> {
  const requestedKeys = [...new Set(input.spaces.map((space) => space.key))];
  const spaces = await backend.listSpaces();
  const idsByKey = new Map(spaces.map((space) => [space.key, space.id]));
  const scope = requestedKeys
    .map((key) => idsByKey.get(key))
    .filter((id): id is string => id !== undefined);

  if (scope.length !== requestedKeys.length || scope.length === 0) {
    return { kind: "none-found", scope };
  }

  const allowedSpaces = new Set(scope);
  const requiredEntities = new Set(input.entities);
  const hits = await backend.query({ scope, cue: input.cue });
  const seen = new Set<string>();
  const candidates: MemoryOwnerCandidate[] = [];

  for (const hit of hits) {
    if (!allowedSpaces.has(hit.spaceId) || seen.has(hit.memoryId)) {
      continue;
    }
    seen.add(hit.memoryId);
    const memory = await backend.readMemory({ memoryId: hit.memoryId });
    if (
      memory.spaceId !== hit.spaceId ||
      ![...requiredEntities].every((entity) =>
        memory.entityRefs.includes(entity),
      )
    ) {
      continue;
    }
    candidates.push({
      memoryId: memory.memoryId,
      spaceId: memory.spaceId,
      revisionId: memory.revisionId,
      ...(memory.documentKey === undefined
        ? {}
        : { documentKey: memory.documentKey }),
    });
  }

  candidates.sort((left, right) =>
    [left.memoryId, left.spaceId]
      .join("\u0000")
      .localeCompare([right.memoryId, right.spaceId].join("\u0000")),
  );
  if (candidates.length === 0) {
    return { kind: "none-found", scope };
  }
  if (candidates.length > 1) {
    return { kind: "ambiguous", candidates };
  }
  const [candidate] = candidates;
  return {
    kind: "existing",
    memoryId: candidate.memoryId,
    spaceId: candidate.spaceId,
    revisionId: candidate.revisionId,
    ...(candidate.documentKey === undefined
      ? {}
      : { documentKey: candidate.documentKey }),
  };
}
