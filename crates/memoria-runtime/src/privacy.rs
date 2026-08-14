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
        ProviderCapability, ProviderEgressPolicy, ProviderTrust, allows_space_provider_policy,
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
}
