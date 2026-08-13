use memoria_types::AuthorityGeneration;

use crate::{CapabilityStatus, DerivedManifest};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedStatus {
    pub authority_generation: AuthorityGeneration,
    pub base_coverage: AuthorityGeneration,
    pub serving_manifest: Option<crate::ManifestId>,
    pub queued_generation: Option<AuthorityGeneration>,
    pub building: bool,
}

impl DerivedStatus {
    pub(crate) fn from_manifest(
        authority_generation: AuthorityGeneration,
        manifest: Option<&DerivedManifest>,
        queued_generation: Option<AuthorityGeneration>,
        building: bool,
    ) -> Self {
        let base = manifest
            .map(|manifest| manifest.capability("base-search"))
            .unwrap_or_else(|| CapabilityStatus::new(false, AuthorityGeneration::initial()));
        Self {
            authority_generation,
            base_coverage: base.coverage(),
            serving_manifest: manifest.map(DerivedManifest::id),
            queued_generation,
            building,
        }
    }
}
