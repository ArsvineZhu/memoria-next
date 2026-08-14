use std::fmt;

use memoria_authority::{SpaceProviderMode, SpaceProviderPolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCapability {
    Embedding,
    Rerank,
    Enrichment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderTrust {
    Local,
    External,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRoute {
    pub capability: ProviderCapability,
    pub trust: ProviderTrust,
    pub signature: String,
}

impl ProviderRoute {
    #[must_use]
    pub fn new(
        capability: ProviderCapability,
        trust: ProviderTrust,
        signature: impl Into<String>,
    ) -> Self {
        Self {
            capability,
            trust,
            signature: signature.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRouteConfig {
    pub embedding: ProviderRoute,
    pub rerank: ProviderRoute,
    pub enrichment: ProviderRoute,
    pub embedding_available: bool,
    pub rerank_available: bool,
    pub enrichment_available: bool,
}

impl Default for ProviderRouteConfig {
    fn default() -> Self {
        Self {
            embedding: ProviderRoute::new(
                ProviderCapability::Embedding,
                ProviderTrust::External,
                "memoria-provider-route-embedding-v1",
            ),
            rerank: ProviderRoute::new(
                ProviderCapability::Rerank,
                ProviderTrust::External,
                "memoria-provider-route-rerank-v1",
            ),
            enrichment: ProviderRoute::new(
                ProviderCapability::Enrichment,
                ProviderTrust::External,
                "memoria-provider-route-enrichment-v1",
            ),
            embedding_available: true,
            rerank_available: true,
            enrichment_available: true,
        }
    }
}

impl ProviderRouteConfig {
    #[must_use]
    pub const fn route(&self, capability: ProviderCapability) -> &ProviderRoute {
        match capability {
            ProviderCapability::Embedding => &self.embedding,
            ProviderCapability::Rerank => &self.rerank,
            ProviderCapability::Enrichment => &self.enrichment,
        }
    }

    #[must_use]
    pub const fn available(&self, capability: ProviderCapability) -> bool {
        match capability {
            ProviderCapability::Embedding => self.embedding_available,
            ProviderCapability::Rerank => self.rerank_available,
            ProviderCapability::Enrichment => self.enrichment_available,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProviderRouteDecision {
    Allowed(ProviderRoute),
    RequiredDenied,
    PreferredDegraded,
}

#[must_use]
pub fn resolve_provider_route(
    policy: SpaceProviderPolicy,
    route: &ProviderRoute,
    required: bool,
) -> ProviderRouteDecision {
    if allows_space_provider_policy(policy, route.capability, route.trust) {
        ProviderRouteDecision::Allowed(route.clone())
    } else if required {
        ProviderRouteDecision::RequiredDenied
    } else {
        ProviderRouteDecision::PreferredDegraded
    }
}

#[must_use]
pub const fn allows_space_provider_mode(mode: SpaceProviderMode, trust: ProviderTrust) -> bool {
    match (mode, trust) {
        (SpaceProviderMode::Deny, _) => false,
        (SpaceProviderMode::LocalOnly, ProviderTrust::Local) => true,
        (SpaceProviderMode::LocalOnly, ProviderTrust::External) => false,
        (SpaceProviderMode::ExternalAllowed, _) => true,
    }
}

#[must_use]
pub const fn allows_space_provider_policy(
    policy: SpaceProviderPolicy,
    capability: ProviderCapability,
    trust: ProviderTrust,
) -> bool {
    let mode = match capability {
        ProviderCapability::Embedding => policy.embedding,
        ProviderCapability::Rerank => policy.reranking,
        ProviderCapability::Enrichment => policy.enrichment,
    };
    allows_space_provider_mode(mode, trust)
}

#[must_use]
pub const fn space_provider_mode(
    policy: SpaceProviderPolicy,
    capability: ProviderCapability,
) -> SpaceProviderMode {
    match capability {
        ProviderCapability::Embedding => policy.embedding,
        ProviderCapability::Rerank => policy.reranking,
        ProviderCapability::Enrichment => policy.enrichment,
    }
}

impl fmt::Display for ProviderCapability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Embedding => "embedding",
            Self::Rerank => "rerank",
            Self::Enrichment => "enrichment",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderEgressPolicy {
    pub allow_embedding: bool,
    pub allow_rerank: bool,
    pub allow_enrichment: bool,
}

impl Default for ProviderEgressPolicy {
    fn default() -> Self {
        Self {
            allow_embedding: true,
            allow_rerank: true,
            allow_enrichment: true,
        }
    }
}

impl ProviderEgressPolicy {
    #[must_use]
    pub const fn allows(self, capability: ProviderCapability) -> bool {
        match capability {
            ProviderCapability::Embedding => self.allow_embedding,
            ProviderCapability::Rerank => self.allow_rerank,
            ProviderCapability::Enrichment => self.allow_enrichment,
        }
    }
}

#[cfg(test)]
mod tests {
    use memoria_authority::{SpaceProviderMode, SpaceProviderPolicy};

    use super::{
        ProviderCapability, ProviderEgressPolicy, ProviderRoute, ProviderRouteConfig,
        ProviderRouteDecision, ProviderTrust, allows_space_provider_policy, resolve_provider_route,
    };

    #[test]
    fn permissions_are_independent() {
        let policy = ProviderEgressPolicy {
            allow_embedding: false,
            allow_rerank: true,
            allow_enrichment: false,
        };

        assert!(!policy.allows(ProviderCapability::Embedding));
        assert!(policy.allows(ProviderCapability::Rerank));
        assert!(!policy.allows(ProviderCapability::Enrichment));
    }

    #[test]
    fn local_only_policy_rejects_external_routes_but_allows_local_routes() {
        let policy = SpaceProviderPolicy {
            embedding: SpaceProviderMode::LocalOnly,
            ..SpaceProviderPolicy::default()
        };

        assert!(!allows_space_provider_policy(
            policy,
            ProviderCapability::Embedding,
            ProviderTrust::External,
        ));
        assert!(allows_space_provider_policy(
            policy,
            ProviderCapability::Embedding,
            ProviderTrust::Local,
        ));
    }

    #[test]
    fn route_resolution_distinguishes_required_and_preferred_denial() {
        let policy = SpaceProviderPolicy {
            embedding: SpaceProviderMode::LocalOnly,
            ..SpaceProviderPolicy::default()
        };
        let route = ProviderRoute::new(
            ProviderCapability::Embedding,
            ProviderTrust::External,
            "test-route",
        );

        assert_eq!(
            resolve_provider_route(policy, &route, true),
            ProviderRouteDecision::RequiredDenied
        );
        assert_eq!(
            resolve_provider_route(policy, &route, false),
            ProviderRouteDecision::PreferredDegraded
        );
    }

    #[test]
    fn default_routes_are_capability_scoped() {
        let routes = ProviderRouteConfig::default();
        assert_eq!(
            routes.route(ProviderCapability::Embedding).capability,
            ProviderCapability::Embedding
        );
        assert_eq!(
            routes.route(ProviderCapability::Rerank).capability,
            ProviderCapability::Rerank
        );
        assert_eq!(
            routes.route(ProviderCapability::Enrichment).capability,
            ProviderCapability::Enrichment
        );
    }
}
