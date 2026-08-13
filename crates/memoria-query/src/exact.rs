use memoria_types::{MemoryId, RevisionId, SpaceId};

use crate::EntityRef;
use crate::compile::CompiledQuery;
use crate::evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    PropagationEvidence, RelationEvidence, SemanticEvidence, TagEvidence,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ExactRecord {
    pub target: CandidateTarget,
    pub text: String,
    pub entity_refs: Vec<EntityRef>,
    pub tags: Vec<String>,
    pub current: bool,
    pub retired: bool,
    pub relations: Vec<String>,
}

impl ExactRecord {
    #[must_use]
    pub fn new(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        text: impl Into<String>,
    ) -> Self {
        Self {
            target: CandidateTarget {
                space_id,
                memory_id,
                revision_id,
            },
            text: text.into(),
            entity_refs: Vec::new(),
            tags: Vec::new(),
            current: true,
            retired: false,
            relations: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_entities(mut self, entities: Vec<EntityRef>) -> Self {
        self.entity_refs = entities;
        self
    }

    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    #[must_use]
    pub fn with_current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    #[must_use]
    pub fn with_retired(mut self, retired: bool) -> Self {
        self.retired = retired;
        self
    }

    #[must_use]
    pub fn with_relations(mut self, relations: Vec<String>) -> Self {
        self.relations = relations;
        self
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExactIndex {
    records: Vec<ExactRecord>,
}

impl ExactIndex {
    #[must_use]
    pub fn new(records: Vec<ExactRecord>) -> Self {
        Self { records }
    }

    #[must_use]
    pub fn records(&self) -> &[ExactRecord] {
        &self.records
    }
}

pub fn execute_exact(compiled: &CompiledQuery, index: &ExactIndex) -> CandidateResponse {
    let mut results = index
        .records
        .iter()
        .filter(|record| matches_query(compiled, record))
        .map(to_evidence)
        .take(compiled.query.budget.max_results)
        .collect::<Vec<_>>();
    results.sort_by_key(|result| result.target.memory_id);
    CandidateResponse {
        results,
        execution: compiled.execution.clone(),
        snapshot: compiled.snapshot.clone(),
    }
}

fn matches_query(compiled: &CompiledQuery, record: &ExactRecord) -> bool {
    let query = &compiled.query;
    query.scope.spaces.contains(&record.target.space_id)
        && record.current
        && !record.retired
        && query
            .constraints
            .memories
            .iter()
            .all(|memory| memory == &record.target.memory_id)
        && query
            .constraints
            .entities
            .iter()
            .all(|entity| record.entity_refs.contains(entity))
        && query
            .constraints
            .tags
            .iter()
            .all(|tag| record.tags.iter().any(|candidate| candidate == tag))
}

fn to_evidence(record: &ExactRecord) -> CandidateEvidence {
    let exact = vec![
        ExactEvidence {
            field: "memory_id".to_owned(),
            value: record.target.memory_id.to_string(),
        },
        ExactEvidence {
            field: "revision_id".to_owned(),
            value: record.target.revision_id.to_string(),
        },
    ];
    CandidateEvidence {
        target: record.target,
        exact,
        lexical: Vec::new(),
        semantic: Vec::<SemanticEvidence>::new(),
        tags: record
            .tags
            .iter()
            .cloned()
            .map(|value| TagEvidence { value })
            .collect(),
        propagation: Vec::<PropagationEvidence>::new(),
        relations: record
            .relations
            .iter()
            .cloned()
            .map(|relation| RelationEvidence { relation })
            .collect(),
        history: Vec::<HistoryEvidence>::new(),
        text: record.text.clone(),
        entity_refs: record.entity_refs.clone(),
    }
}
