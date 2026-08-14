import { asSpaceId, type SpaceId } from "./ids.js";
import type {
  NativeSpaceProviderMode,
  NativeSpaceProviderPolicy,
} from "../native/protocol.js";

export type SpaceProviderMode = NativeSpaceProviderMode;
export type SpaceProviderPolicy = NativeSpaceProviderPolicy;

export const DEFAULT_SPACE_PROVIDER_POLICY: SpaceProviderPolicy = {
  embedding: "external-allowed",
  reranking: "external-allowed",
  enrichment: "external-allowed",
};

export interface Space {
  id: SpaceId;
  key: string;
  providerPolicy: SpaceProviderPolicy;
}

export interface CreateSpaceInput {
  key: string;
  providerPolicy?: SpaceProviderPolicy;
}

interface SpaceEngine {
  createSpace(
    key: string,
    providerPolicy?: SpaceProviderPolicy,
  ): Promise<string>;
}

export interface SpacesApi {
  create(input: CreateSpaceInput): Promise<Space>;
}

export function createSpacesApi(engine: SpaceEngine): SpacesApi {
  return {
    async create(input) {
      const providerPolicy =
        input.providerPolicy ?? DEFAULT_SPACE_PROVIDER_POLICY;
      const id = await engine.createSpace(input.key, providerPolicy);
      return { id: asSpaceId(id), key: input.key, providerPolicy };
    },
  };
}
