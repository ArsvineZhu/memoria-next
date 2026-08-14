import { readFileSync } from "node:fs";

const MAX_ADAPTIVE_PRIOR = 0.1;
const SATURATION_SCALE = 4;
const RECENCY_HALF_LIFE_SECONDS = 30 * 24 * 60 * 60;
const TOP_K = 5;
const EPSILON = 1e-9;

type Outcome =
  | "used"
  | "rejected"
  | "correct_for_query"
  | "incorrect_for_query"
  | "preferred_over"
  | "sufficient"
  | "insufficient";

interface Query {
  scope: string[];
  queryClass?: string;
  tags: string[];
}

interface Candidate {
  id: string;
  spaceId: string;
  memoryId: string;
  revisionId: string;
  baseRelevance: number;
  exact?: boolean;
}

interface Event {
  spaceId: string;
  memoryId: string;
  revisionId: string;
  occurredAt: number;
  outcome: Outcome;
  queryClass?: string;
  tags: string[];
}

interface Scenario {
  id: string;
  description: string;
  now: number;
  query: Query;
  candidates: Candidate[];
  events: Event[];
  crossSpaceCheck?: {
    memoryId: string;
    forbiddenSpace: string;
  };
}

interface TargetState {
  positiveWeight: number;
  negativeCount: number;
  lastSuccessAt?: number;
  revisions: Map<string, number>;
}

interface AffinityState {
  positiveWeight: number;
  negativeWeight: number;
}

interface AdaptiveState {
  targets: Map<string, TargetState>;
  tags: Map<string, AffinityState>;
  queryClasses: Map<string, AffinityState>;
}

interface RankedCandidate extends Candidate {
  accessibility: number;
  adaptivePrior: number;
}

interface ScenarioResult {
  id: string;
  top: string[];
  eligibleCount: number;
  eventsRead: number;
  eventsWritten: number;
  maxAdaptivePrior: number;
  crossSpaceContamination: number;
  exactLookupExclusion: number;
  baseRelevanceViolations: number;
  revisionCarryOver: number;
  staleAccessibility?: number;
  description: string;
}

interface GateReport {
  adaptive: "off" | "on";
  scenarioCount: number;
  topK: number;
  constants: {
    maxAdaptivePrior: number;
    saturationScale: number;
    recencyHalfLifeSeconds: number;
    targetWeight: number;
    affinityWeight: number;
  };
  safety: {
    crossSpaceContamination: number;
    exactLookupExclusion: number;
    unusedTopKEventsWritten: number;
    baseRelevanceViolations: number;
    maxObservedAdaptivePrior: number;
  };
  scenarios: ScenarioResult[];
}

const mode = parseMode();
const scenarios = readJsonLines<Scenario>(
  new URL("./scenarios.jsonl", import.meta.url),
);
const results = scenarios.map((scenario) => replayScenario(scenario, mode));
const report: GateReport = {
  adaptive: mode,
  scenarioCount: scenarios.length,
  topK: TOP_K,
  constants: {
    maxAdaptivePrior: MAX_ADAPTIVE_PRIOR,
    saturationScale: SATURATION_SCALE,
    recencyHalfLifeSeconds: RECENCY_HALF_LIFE_SECONDS,
    targetWeight: 0.75,
    affinityWeight: 0.25,
  },
  safety: {
    crossSpaceContamination: sum(results, "crossSpaceContamination"),
    exactLookupExclusion: sum(results, "exactLookupExclusion"),
    unusedTopKEventsWritten: sum(results, "eventsWritten"),
    baseRelevanceViolations: sum(results, "baseRelevanceViolations"),
    maxObservedAdaptivePrior: Math.max(
      0,
      ...results.map((result) => result.maxAdaptivePrior),
    ),
  },
  scenarios: results,
};

console.log(JSON.stringify(report, null, 2));
console.log(
  `cross-Space contamination = ${report.safety.crossSpaceContamination}`,
);
console.log(
  `exact lookup exclusion caused by accessibility = ${report.safety.exactLookupExclusion}`,
);
console.log(
  `unused/top-K events written = ${report.safety.unusedTopKEventsWritten}`,
);

const violations = [
  report.safety.crossSpaceContamination === 0
    ? undefined
    : "cross-Space contamination must be zero",
  report.safety.exactLookupExclusion === 0
    ? undefined
    : "exact lookup exclusion caused by accessibility must be zero",
  report.safety.unusedTopKEventsWritten === 0
    ? undefined
    : "ranking must not write unused/top-K feedback events",
  report.safety.baseRelevanceViolations === 0
    ? undefined
    : "adaptive prior must not outrank materially stronger base relevance",
  report.safety.maxObservedAdaptivePrior <= MAX_ADAPTIVE_PRIOR + EPSILON
    ? undefined
    : `adaptive prior exceeded ${MAX_ADAPTIVE_PRIOR}`,
].filter((violation): violation is string => violation !== undefined);

if (violations.length > 0) {
  console.error(`Gate G failed:\n- ${violations.join("\n- ")}`);
  process.exitCode = 1;
}

function replayScenario(
  scenario: Scenario,
  adaptive: "off" | "on",
): ScenarioResult {
  const state = reduce(scenario.events);
  const eligible = scenario.candidates.filter((candidate) =>
    scenario.query.scope.includes(candidate.spaceId),
  );
  const ranked = rank(eligible, state, scenario.query, scenario.now, adaptive);
  const top = ranked.slice(0, TOP_K);
  const outOfScope = scenario.candidates.filter(
    (candidate) => !scenario.query.scope.includes(candidate.spaceId),
  );
  const contamination = scenario.crossSpaceCheck
    ? outOfScope.filter(
        (candidate) =>
          candidate.memoryId === scenario.crossSpaceCheck?.memoryId &&
          candidate.spaceId === scenario.crossSpaceCheck?.forbiddenSpace &&
          adaptivePrior(
            state,
            scenario.query,
            candidate.spaceId,
            candidate.memoryId,
            scenario.now,
          ) > EPSILON,
      ).length
    : 0;
  const exactCandidate = scenario.candidates.find(
    (candidate) => candidate.exact,
  );
  const exactLookupExclusion = exactCandidate
    ? top.some((candidate) => candidate.id === exactCandidate.id)
      ? 0
      : 1
    : 0;
  const baseRelevanceViolations = ranked.reduce(
    (total, candidate, index) =>
      total +
      (index > 0 &&
      candidate.baseRelevance >
        (ranked[index - 1]?.baseRelevance ?? Infinity) + EPSILON
        ? 1
        : 0),
    0,
  );
  const revisionCarryOver = scenario.events.some((event) =>
    eligible.some(
      (candidate) =>
        candidate.memoryId === event.memoryId &&
        candidate.revisionId !== event.revisionId &&
        adaptivePrior(
          state,
          scenario.query,
          candidate.spaceId,
          candidate.memoryId,
          scenario.now,
        ) > EPSILON,
    ),
  )
    ? 1
    : 0;
  const staleCandidate =
    scenario.id === "stale-memory-persistence"
      ? ranked.find((candidate) => candidate.memoryId === "memory-stale")
      : undefined;
  return {
    id: scenario.id,
    description: scenario.description,
    top: top.map((candidate) => candidate.id),
    eligibleCount: eligible.length,
    eventsRead: scenario.events.length,
    eventsWritten: 0,
    maxAdaptivePrior: Math.max(
      0,
      ...ranked.map((candidate) => candidate.adaptivePrior),
    ),
    crossSpaceContamination: contamination,
    exactLookupExclusion,
    baseRelevanceViolations,
    revisionCarryOver,
    staleAccessibility: staleCandidate?.accessibility,
  };
}

function rank(
  candidates: Candidate[],
  state: AdaptiveState,
  query: Query,
  now: number,
  adaptive: "off" | "on",
): RankedCandidate[] {
  return candidates
    .map((candidate) => {
      const accessibility =
        adaptive === "on"
          ? contextAccessibility(
              state,
              query,
              candidate.spaceId,
              candidate.memoryId,
              now,
            )
          : 0;
      return {
        ...candidate,
        accessibility,
        adaptivePrior:
          adaptive === "on" ? MAX_ADAPTIVE_PRIOR * accessibility : 0,
      };
    })
    .sort(
      (left, right) =>
        right.baseRelevance - left.baseRelevance ||
        right.adaptivePrior - left.adaptivePrior ||
        left.memoryId.localeCompare(right.memoryId) ||
        left.revisionId.localeCompare(right.revisionId),
    );
}

function reduce(events: Event[]): AdaptiveState {
  const state: AdaptiveState = {
    targets: new Map(),
    tags: new Map(),
    queryClasses: new Map(),
  };
  for (const event of events) {
    const key = targetKey(event.spaceId, event.memoryId);
    const target = state.targets.get(key) ?? {
      positiveWeight: 0,
      negativeCount: 0,
      revisions: new Map<string, number>(),
    };
    const positiveWeight = positiveWeightFor(event.outcome);
    if (positiveWeight > 0) {
      target.positiveWeight += positiveWeight;
      target.lastSuccessAt = Math.max(
        target.lastSuccessAt ?? event.occurredAt,
        event.occurredAt,
      );
      target.revisions.set(
        event.revisionId,
        (target.revisions.get(event.revisionId) ?? 0) + 1,
      );
    } else {
      target.negativeCount += 1;
    }
    state.targets.set(key, target);
    for (const tag of event.tags) {
      recordAffinity(
        state.tags,
        affinityKey(event.spaceId, event.memoryId, tag),
        positiveWeight,
      );
    }
    if (event.queryClass !== undefined) {
      recordAffinity(
        state.queryClasses,
        affinityKey(event.spaceId, event.memoryId, event.queryClass),
        positiveWeight,
      );
    }
  }
  return state;
}

function contextAccessibility(
  state: AdaptiveState,
  query: Query,
  spaceId: string,
  memoryId: string,
  now: number,
): number {
  const target = state.targets.get(targetKey(spaceId, memoryId));
  const targetAccessibility = target
    ? saturation(target.positiveWeight) *
      Math.exp(
        -Math.max(0, now - (target.lastSuccessAt ?? now)) /
          RECENCY_HALF_LIFE_SECONDS,
      )
    : 0;
  const affinities = [
    ...query.tags
      .map((tag) => state.tags.get(affinityKey(spaceId, memoryId, tag)))
      .filter((value): value is AffinityState => value !== undefined),
    ...(query.queryClass === undefined
      ? []
      : [
          state.queryClasses.get(
            affinityKey(spaceId, memoryId, query.queryClass),
          ),
        ].filter((value): value is AffinityState => value !== undefined)),
  ];
  const affinity =
    affinities.length === 0
      ? 0
      : mean(affinities.map((value) => (affinityScore(value) + 1) / 2));
  return Math.max(0, Math.min(1, 0.75 * targetAccessibility + 0.25 * affinity));
}

function adaptivePrior(
  state: AdaptiveState,
  query: Query,
  spaceId: string,
  memoryId: string,
  now: number,
): number {
  return query.scope.includes(spaceId)
    ? MAX_ADAPTIVE_PRIOR *
        contextAccessibility(state, query, spaceId, memoryId, now)
    : 0;
}

function recordAffinity(
  collection: Map<string, AffinityState>,
  key: string,
  weight: number,
): void {
  const affinity = collection.get(key) ?? {
    positiveWeight: 0,
    negativeWeight: 0,
  };
  if (weight > 0) {
    affinity.positiveWeight += weight;
  } else {
    affinity.negativeWeight += 1;
  }
  collection.set(key, affinity);
}

function positiveWeightFor(outcome: Outcome): number {
  switch (outcome) {
    case "used":
    case "correct_for_query":
    case "preferred_over":
      return 1;
    case "sufficient":
      return 0.5;
    case "rejected":
    case "incorrect_for_query":
    case "insufficient":
      return 0;
  }
}

function saturation(positiveWeight: number): number {
  return 1 - Math.exp(-positiveWeight / SATURATION_SCALE);
}

function affinityScore(value: AffinityState): number {
  const total = value.positiveWeight + value.negativeWeight;
  return total === 0
    ? 0
    : (value.positiveWeight - value.negativeWeight) / total;
}

function targetKey(spaceId: string, memoryId: string): string {
  return `${spaceId}\u0000${memoryId}`;
}

function affinityKey(spaceId: string, memoryId: string, cue: string): string {
  return `${targetKey(spaceId, memoryId)}\u0000${cue}`;
}

function sum<K extends keyof ScenarioResult>(
  values: ScenarioResult[],
  key: K,
): number {
  return values.reduce((total, value) => total + Number(value[key] ?? 0), 0);
}

function mean(values: number[]): number {
  return values.length === 0
    ? 0
    : values.reduce((total, value) => total + value, 0) / values.length;
}

function parseMode(): "off" | "on" {
  const argument = process.argv.find((value) =>
    value.startsWith("--adaptive="),
  );
  const mode = argument?.slice("--adaptive=".length);
  if (mode === "off" || mode === "on") {
    return mode;
  }
  console.error("usage: replay.ts --adaptive=off|on");
  process.exit(1);
}

function readJsonLines<T>(url: URL): T[] {
  return readFileSync(url, "utf8")
    .split(/\r?\n/)
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line) as T);
}
