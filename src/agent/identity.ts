export interface ConversationBinding {
  readonly surface: string;
  readonly entityRef: string;
}

export interface HostIdentity {
  readonly entityRef: string;
  readonly surface?: string;
}

export interface EntityObservation {
  readonly entityRef: string;
  readonly surface: string;
  readonly spaceId: string;
}

export interface IdentityCandidate {
  readonly entityRef: string;
  readonly surface: string;
  readonly spaceId?: string;
  readonly source: "conversation" | "directory" | "observation";
}

export interface IdentityResolutionRequest {
  readonly surface: string;
  readonly spaces: readonly string[];
}

export type IdentityResolution =
  | {
      readonly status: "resolved";
      readonly entityRef: string;
      readonly source: "conversation" | "directory";
      readonly candidate: IdentityCandidate;
    }
  | {
      readonly status: "ambiguous";
      readonly reason:
        "multiple-authoritative" | "observation-requires-confirmation";
      readonly candidates: readonly IdentityCandidate[];
    }
  | {
      readonly status: "not-found";
      readonly candidates: readonly [];
    };

export interface IdentityDirectory {
  find(surface: string): Promise<readonly HostIdentity[]>;
}

export interface ObservationSource {
  discover(input: {
    surface: string;
    spaces: readonly string[];
  }): Promise<readonly EntityObservation[]>;
}

export interface IdentityResolverOptions {
  conversationBindings:
    | readonly ConversationBinding[]
    | (() =>
        | readonly ConversationBinding[]
        | Promise<readonly ConversationBinding[]>);
  directory: IdentityDirectory;
  observations: ObservationSource;
}

export interface IdentityResolver {
  resolve(request: IdentityResolutionRequest): Promise<IdentityResolution>;
}

export function createIdentityResolver(
  options: IdentityResolverOptions,
): IdentityResolver {
  return new DefaultIdentityResolver(options);
}

class DefaultIdentityResolver implements IdentityResolver {
  constructor(private readonly options: IdentityResolverOptions) {}

  async resolve(
    request: IdentityResolutionRequest,
  ): Promise<IdentityResolution> {
    const conversation = await this.readConversationBindings();
    const conversationCandidates = uniqueCandidates(
      conversation
        .filter((binding) => binding.surface === request.surface)
        .map((binding) => ({
          entityRef: binding.entityRef,
          surface: binding.surface,
          source: "conversation" as const,
        })),
    );
    if (conversationCandidates.length === 1) {
      const [candidate] = conversationCandidates;
      return {
        status: "resolved",
        entityRef: candidate.entityRef,
        source: "conversation",
        candidate,
      };
    }
    if (conversationCandidates.length > 1) {
      return {
        status: "ambiguous",
        reason: "multiple-authoritative",
        candidates: conversationCandidates,
      };
    }

    const directoryCandidates = uniqueCandidates(
      (await this.options.directory.find(request.surface)).map((identity) => ({
        entityRef: identity.entityRef,
        surface: identity.surface ?? request.surface,
        source: "directory" as const,
      })),
    );
    if (directoryCandidates.length === 1) {
      const [candidate] = directoryCandidates;
      return {
        status: "resolved",
        entityRef: candidate.entityRef,
        source: "directory",
        candidate,
      };
    }
    if (directoryCandidates.length > 1) {
      return {
        status: "ambiguous",
        reason: "multiple-authoritative",
        candidates: directoryCandidates,
      };
    }

    const allowedSpaces = new Set(request.spaces);
    const observedCandidates = uniqueCandidates(
      (
        await this.options.observations.discover({
          surface: request.surface,
          spaces: [...request.spaces],
        })
      )
        .filter(
          (observation) =>
            observation.surface === request.surface &&
            allowedSpaces.has(observation.spaceId),
        )
        .map((observation) => ({
          entityRef: observation.entityRef,
          surface: observation.surface,
          spaceId: observation.spaceId,
          source: "observation" as const,
        })),
    );
    if (observedCandidates.length > 0) {
      return {
        status: "ambiguous",
        reason: "observation-requires-confirmation",
        candidates: observedCandidates,
      };
    }

    return { status: "not-found", candidates: [] };
  }

  private async readConversationBindings(): Promise<
    readonly ConversationBinding[]
  > {
    const value = this.options.conversationBindings;
    return typeof value === "function" ? await value() : value;
  }
}

function uniqueCandidates(
  candidates: readonly IdentityCandidate[],
): IdentityCandidate[] {
  const seen = new Set<string>();
  return candidates
    .filter((candidate) => {
      const key = [
        candidate.entityRef,
        candidate.surface,
        candidate.spaceId ?? "",
        candidate.source,
      ].join("\u0000");
      if (seen.has(key)) {
        return false;
      }
      seen.add(key);
      return true;
    })
    .sort((left, right) => {
      const leftKey = [left.entityRef, left.spaceId ?? "", left.source].join(
        "\u0000",
      );
      const rightKey = [
        right.entityRef,
        right.spaceId ?? "",
        right.source,
      ].join("\u0000");
      return leftKey.localeCompare(rightKey);
    });
}
