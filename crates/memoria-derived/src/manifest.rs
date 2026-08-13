use memoria_types::AuthorityGeneration;
use std::fmt;

use crate::artifact::ArtifactId;

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
}

impl DerivedManifest {
    pub(crate) fn new(
        id: ManifestId,
        authority_generation: AuthorityGeneration,
        artifacts: Vec<ArtifactId>,
    ) -> Self {
        Self {
            id,
            authority_generation,
            artifacts,
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
}
