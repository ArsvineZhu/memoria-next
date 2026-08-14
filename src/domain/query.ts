import type { MemoryId, RevisionId, SpaceId } from "./ids.js";

export type EntityRef = string;

export interface MemoryReferenceInput {
  memoryId: MemoryId;
  revisionId?: RevisionId;
  nodeId?: string;
}

export type QueryOptionalCapability =
  "semantic" | "associative" | "reranking" | "adaptive";

interface MemoryQueryCue {
  text?: string;
  tags?: string[];
  entities?: EntityRef[];
  memories?: MemoryReferenceInput[];
}

interface MemoryQueryConstraints {
  tags?: string[];
  entities?: EntityRef[];
  memories?: MemoryReferenceInput[];
  lifecycle?: "active" | "retired" | "any";
}

interface MemoryQueryHistory {
  mode?: "current" | "all-revisions" | "changes";
  fromAuthorityGeneration?: string;
  toAuthorityGeneration?: string;
}

type AuthorityConsistency =
  | { mode: "latest" }
  | { mode: "at-least"; generation: string }
  | { mode: "exact"; generation: string };

interface MemoryQueryConsistency {
  authority?: AuthorityConsistency;
  required?: QueryOptionalCapability[];
  preferred?: QueryOptionalCapability[];
  onNotReady?: "fail" | "wait";
  timeoutMs?: number;
}

interface MemoryQueryBudget {
  maxResults?: number;
  maxMatchesPerResult?: number;
  maxEvidenceTokens?: number;
}

export interface MemoryQueryInput {
  scope: {
    spaces: Array<SpaceId | { id: SpaceId }>;
  };

  cue?: MemoryQueryCue;
  constraints?: MemoryQueryConstraints;
  temporal?: { validAt?: string };
  history?: MemoryQueryHistory;
  consistency?: MemoryQueryConsistency;
  budget?: MemoryQueryBudget;
  quality?: "fast" | "balanced" | "thorough";
}

interface NormalizedHistory {
  mode: "current" | "all-revisions" | "changes";
  fromAuthorityGeneration?: string;
  toAuthorityGeneration?: string;
}

interface NormalizedConsistency {
  authority: AuthorityConsistency;
  required: QueryOptionalCapability[];
  preferred: QueryOptionalCapability[];
  onNotReady: "fail" | "wait";
  timeoutMs: number;
}

interface NormalizedBudget {
  maxResults: number;
  maxMatchesPerResult: number;
  maxEvidenceTokens: number;
}

export interface NormalizedMemoryQuery {
  scope: {
    spaces: string[];
  };
  cue?: MemoryQueryCue;
  constraints?: MemoryQueryConstraints;
  temporal?: { validAt?: string };
  history: NormalizedHistory;
  consistency: NormalizedConsistency;
  budget: NormalizedBudget;
  quality: "fast" | "balanced" | "thorough";
}

const optionalCapabilities = new Set<QueryOptionalCapability>([
  "semantic",
  "associative",
  "reranking",
  "adaptive",
]);

function normalizeCue(cue: MemoryQueryCue): MemoryQueryCue {
  return {
    ...(cue.text === undefined ? {} : { text: cue.text }),
    ...(cue.tags === undefined ? {} : { tags: [...cue.tags] }),
    ...(cue.entities === undefined ? {} : { entities: [...cue.entities] }),
    ...(cue.memories === undefined ? {} : { memories: [...cue.memories] }),
  };
}

function normalizeConstraints(
  constraints: MemoryQueryConstraints,
): MemoryQueryConstraints {
  return {
    ...(constraints.tags === undefined ? {} : { tags: [...constraints.tags] }),
    ...(constraints.entities === undefined
      ? {}
      : { entities: [...constraints.entities] }),
    ...(constraints.memories === undefined
      ? {}
      : { memories: [...constraints.memories] }),
    ...(constraints.lifecycle === undefined
      ? {}
      : { lifecycle: constraints.lifecycle }),
  };
}

function normalizeCapabilities(
  required: QueryOptionalCapability[],
  preferred: QueryOptionalCapability[],
): {
  required: QueryOptionalCapability[];
  preferred: QueryOptionalCapability[];
} {
  const seen = new Set<string>();
  for (const capability of [...required, ...preferred]) {
    if (!optionalCapabilities.has(capability)) {
      throw new Error(`unsupported query capability: ${String(capability)}`);
    }
    if (seen.has(capability)) {
      throw new Error(`duplicate query capability: ${capability}`);
    }
    seen.add(capability);
  }
  return { required: [...required], preferred: [...preferred] };
}

function assertNonNegativeNumber(value: number, name: string): void {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new Error(`${name} must be non-negative`);
  }
}

function assertPositiveNumber(value: number, name: string): void {
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw new Error(`${name} must be positive`);
  }
}

export function normalizeQuery(input: MemoryQueryInput): NormalizedMemoryQuery {
  const spaces = input.scope?.spaces;
  if (!spaces || spaces.length === 0) {
    throw new Error("scope.spaces must contain at least one SpaceId");
  }

  const consistency = input.consistency;
  const capabilities = normalizeCapabilities(
    consistency?.required ?? [],
    consistency?.preferred ?? [],
  );
  const timeoutMs = consistency?.timeoutMs ?? 5000;
  assertNonNegativeNumber(timeoutMs, "timeoutMs");

  const budget = input.budget;
  const maxResults = budget?.maxResults ?? 10;
  const maxMatchesPerResult = budget?.maxMatchesPerResult ?? 3;
  const maxEvidenceTokens = budget?.maxEvidenceTokens ?? 1500;
  assertPositiveNumber(maxResults, "maxResults");
  assertPositiveNumber(maxMatchesPerResult, "maxMatchesPerResult");
  assertPositiveNumber(maxEvidenceTokens, "maxEvidenceTokens");

  return {
    scope: {
      spaces: spaces.map((space) =>
        typeof space === "string" ? space : space.id,
      ),
    },
    ...(input.cue === undefined ? {} : { cue: normalizeCue(input.cue) }),
    ...(input.constraints === undefined
      ? {}
      : { constraints: normalizeConstraints(input.constraints) }),
    ...(input.temporal === undefined
      ? {}
      : { temporal: { ...input.temporal } }),
    history: {
      mode: input.history?.mode ?? "current",
      ...(input.history?.fromAuthorityGeneration === undefined
        ? {}
        : { fromAuthorityGeneration: input.history.fromAuthorityGeneration }),
      ...(input.history?.toAuthorityGeneration === undefined
        ? {}
        : { toAuthorityGeneration: input.history.toAuthorityGeneration }),
    },
    consistency: {
      authority: consistency?.authority ?? { mode: "latest" },
      required: capabilities.required,
      preferred: capabilities.preferred,
      onNotReady: consistency?.onNotReady ?? "fail",
      timeoutMs,
    },
    budget: {
      maxResults,
      maxMatchesPerResult,
      maxEvidenceTokens,
    },
    quality: input.quality ?? "balanced",
  };
}
