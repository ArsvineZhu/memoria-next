use memoria_types::{AuthorityGeneration, MemoryId, RevisionId, SpaceId};

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
    pub purged: bool,
    pub authority_generation: AuthorityGeneration,
    pub valid_until: Option<AuthorityGeneration>,
    pub node_ids: Vec<String>,
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
            purged: false,
            authority_generation: AuthorityGeneration::initial(),
            valid_until: None,
            node_ids: Vec::new(),
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
    pub fn with_purged(mut self, purged: bool) -> Self {
        self.purged = purged;
        self
    }

    #[must_use]
    pub fn with_authority_generation(mut self, generation: AuthorityGeneration) -> Self {
        self.authority_generation = generation;
        self
    }

    #[must_use]
    pub fn with_valid_until(mut self, generation: AuthorityGeneration) -> Self {
        self.valid_until = Some(generation);
        self
    }

    #[must_use]
    pub fn with_node_ids(mut self, node_ids: Vec<String>) -> Self {
        self.node_ids = node_ids;
        self
    }

    #[must_use]
    pub fn with_relations(mut self, relations: Vec<String>) -> Self {
        self.relations = relations;
        self
    }

    pub fn memory_reference_ids(&self) -> impl Iterator<Item = &str> {
        self.relations
            .iter()
            .filter_map(|relation| relation.strip_prefix("memory-ref:"))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExactIndex {
    records: Vec<ExactRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryReference {
    pub memory_id: MemoryId,
    pub revision_id: Option<RevisionId>,
    pub node_id: Option<String>,
}

impl MemoryReference {
    #[must_use]
    pub const fn logical(memory_id: MemoryId) -> Self {
        Self {
            memory_id,
            revision_id: None,
            node_id: None,
        }
    }

    #[must_use]
    pub const fn revision(memory_id: MemoryId, revision_id: RevisionId) -> Self {
        Self {
            memory_id,
            revision_id: Some(revision_id),
            node_id: None,
        }
    }

    #[must_use]
    pub fn node(mut self, node_id: impl Into<String>) -> Self {
        self.node_id = Some(node_id.into());
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReferenceStatus {
    Resolved,
    Retired,
    Purged,
    Unresolved,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedReference {
    pub target: Option<CandidateTarget>,
    pub node_id: Option<String>,
    pub status: ReferenceStatus,
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

    #[must_use]
    pub fn record_for_target(&self, target: CandidateTarget) -> Option<&ExactRecord> {
        self.records.iter().find(|record| record.target == target)
    }

    #[must_use]
    pub fn resolve(
        &self,
        reference: &MemoryReference,
        authority_generation: AuthorityGeneration,
    ) -> ResolvedReference {
        match reference.revision_id {
            Some(revision_id) => self.resolve_revision(
                reference.memory_id,
                revision_id,
                reference.node_id.as_deref(),
                authority_generation,
            ),
            None => self.resolve_logical(
                reference.memory_id,
                reference.node_id.as_deref(),
                authority_generation,
            ),
        }
    }

    #[must_use]
    pub fn resolve_logical(
        &self,
        memory_id: MemoryId,
        node_id: Option<&str>,
        authority_generation: AuthorityGeneration,
    ) -> ResolvedReference {
        let record = self
            .records
            .iter()
            .filter(|record| {
                record.target.memory_id == memory_id
                    && record.current
                    && visible_at(record, authority_generation)
            })
            .max_by_key(|record| record.authority_generation);
        resolve_record(record, node_id, false)
    }

    #[must_use]
    pub fn resolve_revision(
        &self,
        memory_id: MemoryId,
        revision_id: RevisionId,
        node_id: Option<&str>,
        authority_generation: AuthorityGeneration,
    ) -> ResolvedReference {
        let record = self.records.iter().find(|record| {
            record.target.memory_id == memory_id
                && record.target.revision_id == revision_id
                && visible_at(record, authority_generation)
        });
        resolve_record(record, node_id, true)
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

pub(crate) fn matches_query(compiled: &CompiledQuery, record: &ExactRecord) -> bool {
    let query = &compiled.query;
    query.scope.spaces.contains(&record.target.space_id)
        && record.current
        && !record.retired
        && !record.purged
        && visible_at(record, compiled.snapshot.authority_generation)
        && (query.cue.memories.is_empty()
            || query.cue.memories.iter().any(|reference| {
                reference.memory_id == record.target.memory_id
                    && reference
                        .revision_id
                        .is_none_or(|revision_id| revision_id == record.target.revision_id)
            }))
        && query
            .constraints
            .memories
            .iter()
            .all(|memory| memory.memory_id == record.target.memory_id)
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

pub(crate) fn visible_at(record: &ExactRecord, generation: AuthorityGeneration) -> bool {
    record.authority_generation <= generation
        && record
            .valid_until
            .is_none_or(|valid_until| generation < valid_until)
}

fn resolve_record(
    record: Option<&ExactRecord>,
    node_id: Option<&str>,
    historical: bool,
) -> ResolvedReference {
    let Some(record) = record else {
        return ResolvedReference {
            target: None,
            node_id: node_id.map(str::to_owned),
            status: ReferenceStatus::Unresolved,
        };
    };
    if record.purged {
        return ResolvedReference {
            target: Some(record.target),
            node_id: node_id.map(str::to_owned),
            status: ReferenceStatus::Purged,
        };
    }
    if let Some(node_id) = node_id
        && !record.node_ids.iter().any(|candidate| candidate == node_id)
    {
        return ResolvedReference {
            target: None,
            node_id: Some(node_id.to_owned()),
            status: ReferenceStatus::Unresolved,
        };
    }
    ResolvedReference {
        target: Some(record.target),
        node_id: node_id.map(str::to_owned),
        status: if historical && record.retired {
            ReferenceStatus::Retired
        } else {
            ReferenceStatus::Resolved
        },
    }
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
            .filter(|relation| !relation.starts_with("memory-ref:"))
            .cloned()
            .map(|relation| RelationEvidence { relation })
            .collect(),
        history: Vec::<HistoryEvidence>::new(),
        text: record.text.clone(),
        entity_refs: record.entity_refs.clone(),
    }
}
