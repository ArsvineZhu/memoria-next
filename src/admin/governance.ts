export interface EntityObservation {
  readonly entityRef: string;
  readonly surface: string;
  readonly spaceId: string;
}

export interface ScopedEntityQuery {
  readonly surface: string;
  readonly allowedSpaces: readonly string[];
}

/**
 * Apply the caller scope before counting, ranking, or serializing discovery
 * observations. The returned objects contain no aggregate global metadata.
 */
export function discoverScopedEntities(
  observations: readonly EntityObservation[],
  query: ScopedEntityQuery,
): EntityObservation[] {
  const allowedSpaces = new Set(query.allowedSpaces);
  return observations
    .filter(
      (observation) =>
        observation.surface === query.surface &&
        allowedSpaces.has(observation.spaceId),
    )
    .map((observation) => ({ ...observation }));
}
