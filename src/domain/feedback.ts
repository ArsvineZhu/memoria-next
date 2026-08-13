import { MemoriaError } from "./errors.js";
import {
  asMemoryId,
  asRevisionId,
  asSpaceId,
  type MemoryId,
  type RevisionId,
  type SpaceId,
} from "./ids.js";

export type FeedbackOutcome =
  | "used"
  | "rejected"
  | "correct_for_query"
  | "incorrect_for_query"
  | "preferred_over"
  | "sufficient"
  | "insufficient";

export interface FeedbackInput {
  retrievalId: string;
  idempotencyKey: string;
  events: Array<{
    resultId: string;
    outcome: FeedbackOutcome;
  }>;
}

export interface FeedbackEvent {
  eventId: string;
  generation: string;
  retrievalId: string;
  spaceId: SpaceId;
  memoryId: MemoryId;
  revisionId: RevisionId;
  semanticNodeId?: string;
  outcome: FeedbackOutcome;
  occurredAtSeconds: number;
}

export interface FeedbackCommit {
  generation: string;
  events: FeedbackEvent[];
}

export interface FeedbackApi {
  submit(input: FeedbackInput): Promise<FeedbackCommit>;
}

interface FeedbackEngine {
  submitFeedback(input: FeedbackInput): Promise<{
    generation: string;
    events: Array<{
      eventId: string;
      generation: string;
      retrievalId: string;
      spaceId: string;
      memoryId: string;
      revisionId: string;
      semanticNodeId?: string;
      outcome: string;
      occurredAtSeconds: number;
    }>;
  }>;
}

export function createFeedbackApi(engine: FeedbackEngine): FeedbackApi {
  return {
    async submit(input) {
      if (input.events.length === 0) {
        throw new MemoriaError(
          "ADAPTIVE_ERROR",
          "feedback submission must contain at least one event",
        );
      }
      const committed = await engine.submitFeedback(input);
      return {
        generation: committed.generation,
        events: committed.events.map((event) => ({
          eventId: event.eventId,
          generation: event.generation,
          retrievalId: event.retrievalId,
          spaceId: asSpaceId(event.spaceId),
          memoryId: asMemoryId(event.memoryId),
          revisionId: asRevisionId(event.revisionId),
          ...(event.semanticNodeId === undefined
            ? {}
            : { semanticNodeId: event.semanticNodeId }),
          outcome: parseFeedbackOutcome(event.outcome),
          occurredAtSeconds: event.occurredAtSeconds,
        })),
      };
    },
  };
}

function parseFeedbackOutcome(value: string): FeedbackOutcome {
  const outcomes: readonly FeedbackOutcome[] = [
    "used",
    "rejected",
    "correct_for_query",
    "incorrect_for_query",
    "preferred_over",
    "sufficient",
    "insufficient",
  ];
  if (outcomes.includes(value as FeedbackOutcome)) {
    return value as FeedbackOutcome;
  }
  throw new MemoriaError(
    "ADAPTIVE_ERROR",
    `unknown feedback outcome: ${value}`,
  );
}
