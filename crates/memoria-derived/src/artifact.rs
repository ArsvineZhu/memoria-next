use std::fmt;

use memoria_types::AuthorityGeneration;

use crate::DerivedError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactId(i64);

impl ArtifactId {
    pub(crate) const fn from_raw(value: i64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> i64 {
        self.0
    }
}

impl fmt::Display for ArtifactId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "A_{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactState {
    Planned,
    Building,
    Staged,
    Validated,
    Published,
    Failed,
    Superseded,
    Collected,
}

impl ArtifactState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Building => "building",
            Self::Staged => "staged",
            Self::Validated => "validated",
            Self::Published => "published",
            Self::Failed => "failed",
            Self::Superseded => "superseded",
            Self::Collected => "collected",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, DerivedError> {
        match value {
            "planned" => Ok(Self::Planned),
            "building" => Ok(Self::Building),
            "staged" => Ok(Self::Staged),
            "validated" => Ok(Self::Validated),
            "published" => Ok(Self::Published),
            "failed" => Ok(Self::Failed),
            "superseded" => Ok(Self::Superseded),
            "collected" => Ok(Self::Collected),
            _ => Err(DerivedError::InvalidCatalogState {
                value: value.to_owned(),
            }),
        }
    }

    #[must_use]
    pub const fn can_publish(self) -> bool {
        matches!(self, Self::Validated | Self::Published)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactDescriptor {
    id: ArtifactId,
    kind: String,
    version: u32,
    authority_generation: AuthorityGeneration,
    state: ArtifactState,
}

impl ArtifactDescriptor {
    pub(crate) fn new(
        id: ArtifactId,
        kind: String,
        version: u32,
        authority_generation: AuthorityGeneration,
        state: ArtifactState,
    ) -> Self {
        Self {
            id,
            kind,
            version,
            authority_generation,
            state,
        }
    }

    #[must_use]
    pub const fn id(&self) -> ArtifactId {
        self.id
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub const fn authority_generation(&self) -> AuthorityGeneration {
        self.authority_generation
    }

    #[must_use]
    pub const fn state(&self) -> ArtifactState {
        self.state
    }
}
