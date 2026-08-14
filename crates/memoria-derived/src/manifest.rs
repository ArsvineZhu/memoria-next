use memoria_types::AuthorityGeneration;
use std::fmt;

use crate::artifact::ArtifactId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityStatus {
    ready: bool,
    coverage: AuthorityGeneration,
}

impl CapabilityStatus {
    pub(crate) const fn new(ready: bool, coverage: AuthorityGeneration) -> Self {
        Self { ready, coverage }
    }

    #[must_use]
    pub const fn is_ready(self) -> bool {
        self.ready
    }

    #[must_use]
    pub const fn coverage(self) -> AuthorityGeneration {
        self.coverage
    }

    #[must_use]
    pub const fn serving_coverage(self) -> AuthorityGeneration {
        if self.ready {
            self.coverage
        } else {
            AuthorityGeneration::initial()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ManifestId(i64);

impl ManifestId {
    pub(crate) const fn from_raw(value: i64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl fmt::Display for ManifestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "MF_{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedManifest {
    id: ManifestId,
    authority_generation: AuthorityGeneration,
    artifacts: Vec<ArtifactId>,
    capabilities: Vec<String>,
}

impl DerivedManifest {
    #[must_use]
    pub fn empty_for_lexical_query(authority_generation: AuthorityGeneration) -> Self {
        Self {
            id: ManifestId::from_raw(0),
            authority_generation,
            artifacts: Vec::new(),
            capabilities: Vec::new(),
        }
    }

    pub(crate) fn new(
        id: ManifestId,
        authority_generation: AuthorityGeneration,
        artifacts: Vec<ArtifactId>,
        capabilities: Vec<String>,
    ) -> Self {
        Self {
            id,
            authority_generation,
            artifacts,
            capabilities,
        }
    }

    #[must_use]
    pub const fn id(&self) -> ManifestId {
        self.id
    }

    #[must_use]
    pub const fn authority_generation(&self) -> AuthorityGeneration {
        self.authority_generation
    }

    pub fn artifacts(&self) -> impl Iterator<Item = ArtifactId> + '_ {
        self.artifacts.iter().copied()
    }

    #[must_use]
    pub fn capability(&self, name: &str) -> CapabilityStatus {
        CapabilityStatus::new(
            self.capabilities
                .iter()
                .any(|capability| capability == name),
            self.authority_generation,
        )
    }

    pub fn capabilities(&self) -> impl Iterator<Item = &str> {
        self.capabilities.iter().map(String::as_str)
    }
}
