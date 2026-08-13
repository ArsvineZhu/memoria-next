import { asSpaceId, type SpaceId } from "./ids.js";

export interface Space {
  id: SpaceId;
  key: string;
}

export interface CreateSpaceInput {
  key: string;
}

interface SpaceEngine {
  createSpace(key: string): Promise<string>;
}

export interface SpacesApi {
  create(input: CreateSpaceInput): Promise<Space>;
}

export function createSpacesApi(engine: SpaceEngine): SpacesApi {
  return {
    async create(input) {
      const id = await engine.createSpace(input.key);
      return { id: asSpaceId(id), key: input.key };
    },
  };
}
