import { MemoriaError } from "./errors.js";

export interface FeedbackInput {
  queryId: string;
  accepted: boolean;
  resultId?: string;
}

export interface FeedbackApi {
  submit(input: FeedbackInput): Promise<void>;
}

export function createFeedbackApi(): FeedbackApi {
  return {
    async submit(_input) {
      throw new MemoriaError(
        "UNSUPPORTED_OPERATION",
        "durable feedback is not available until the Adaptive runtime is enabled",
      );
    },
  };
}
