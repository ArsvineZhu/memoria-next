use memoria_types::{MemoryId, RevisionId, SpaceId};

use crate::EntityRef;
use crate::semantic::SemanticResolution;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateTarget {
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactEvidence {
    pub field: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LexicalEvidence {
    pub rank: usize,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticEvidence {
    pub score: f32,
    pub resolution: SemanticResolution,
    pub channel: SemanticChannel,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SemanticChannel {
    Direct,
    Residual,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagEvidence {
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropagationEvidence {
    pub score: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelationEvidence {
    pub relation: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEvidence {
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateEvidence {
    pub target: CandidateTarget,
    pub exact: Vec<ExactEvidence>,
    pub lexical: Vec<LexicalEvidence>,
    pub semantic: Vec<SemanticEvidence>,
    pub tags: Vec<TagEvidence>,
    pub propagation: Vec<PropagationEvidence>,
    pub relations: Vec<RelationEvidence>,
    pub history: Vec<HistoryEvidence>,
    pub text: String,
    pub entity_refs: Vec<EntityRef>,
}

impl CandidateEvidence {
    #[must_use]
    pub fn contains_entity(&self, entity: &EntityRef) -> bool {
        self.entity_refs.iter().any(|candidate| candidate == entity)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CandidateResponse {
    pub results: Vec<CandidateEvidence>,
    pub execution: crate::CapabilityExecution,
    pub snapshot: crate::QuerySnapshot,
}
