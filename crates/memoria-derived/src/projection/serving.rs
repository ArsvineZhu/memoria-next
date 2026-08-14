use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};

use crate::DerivedError;
use crate::projection::ProjectionTarget;
use crate::projection::entities::EntityObservationBuilder;
use crate::projection::lexical::LexicalDocument;
use crate::projection::tags::ExplicitTagBuilder;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServingRecord {
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub authority_generation: AuthorityGeneration,
    pub text: String,
    pub entity_refs: Vec<String>,
    pub tags: Vec<String>,
    pub node_ids: Vec<String>,
    pub current: bool,
    pub retired: bool,
}

impl ServingRecord {
    pub fn from_document(
        document: &LexicalDocument,
        authority_generation: AuthorityGeneration,
    ) -> Result<Self, DerivedError> {
        let target = ProjectionTarget {
            space_id: Some(document.space_id()),
            memory_id: Some(document.memory_id()),
            revision_id: Some(document.revision_id()),
        };
        let entity_refs = EntityObservationBuilder::build_for(document.ir(), target)?
            .observations()
            .iter()
            .map(|observation| observation.entity_ref.as_str().to_owned())
            .collect();
        let tags = ExplicitTagBuilder::build_for(document.ir(), target)?
            .memberships()
            .iter()
            .map(|membership| membership.value.clone())
            .collect();
        let node_ids = document
            .ir()
            .nodes()
            .filter_map(|node| node.id().map(ToString::to_string))
            .collect();
        Ok(Self {
            space_id: document.space_id(),
            memory_id: document.memory_id(),
            revision_id: document.revision_id(),
            authority_generation,
            text: document.lexical_text(),
            entity_refs,
            tags,
            node_ids,
            current: true,
            retired: false,
        })
    }
}
