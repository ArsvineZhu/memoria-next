declare const spaceIdBrand: unique symbol;
declare const memoryIdBrand: unique symbol;
declare const revisionIdBrand: unique symbol;

export type SpaceId = string & { readonly [spaceIdBrand]: "SpaceId" };
export type MemoryId = string & { readonly [memoryIdBrand]: "MemoryId" };
export type RevisionId = string & { readonly [revisionIdBrand]: "RevisionId" };

export function asSpaceId(value: string): SpaceId {
  return value as SpaceId;
}

export function asMemoryId(value: string): MemoryId {
  return value as MemoryId;
}

export function asRevisionId(value: string): RevisionId {
  return value as RevisionId;
}
