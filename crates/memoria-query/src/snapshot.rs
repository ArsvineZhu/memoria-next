use memoria_derived::ManifestId;
use memoria_types::{AdaptiveGeneration, AuthorityGeneration};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum AdaptiveSnapshotIdentity {
    #[default]
    Disabled,
    Enabled {
        generation: AdaptiveGeneration,
        model_version: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuerySnapshot {
    pub authority_generation: AuthorityGeneration,
    pub derived_manifest: ManifestId,
    pub adaptive: AdaptiveSnapshotIdentity,
}
