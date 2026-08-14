use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCapability {
    Embedding,
    Rerank,
    Enrichment,
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
    use super::{ProviderCapability, ProviderEgressPolicy};

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
}
