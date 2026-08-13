use std::collections::{BTreeMap, HashMap};

use memoria_derived::VectorHit;
use memoria_types::AuthorityGeneration;

use crate::compile::CompiledQuery;
use crate::evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    PropagationEvidence, RelationEvidence, SemanticEvidence, TagEvidence,
};
use crate::exact::ExactIndex;
use crate::model::EntityRef;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SemanticResolution {
    Leaf,
    Node,
    Section,
}

impl SemanticResolution {
    #[must_use]
    fn priority(self) -> u8 {
        match self {
            Self::Leaf => 0,
            Self::Node => 1,
            Self::Section => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticCandidate {
    pub target: CandidateTarget,
    pub score: f32,
    pub resolution: SemanticResolution,
    pub text: String,
    pub entity_refs: Vec<EntityRef>,
    pub tags: Vec<String>,
    pub current: bool,
    pub retired: bool,
    pub purged: bool,
    pub authority_generation: AuthorityGeneration,
    pub valid_until: Option<AuthorityGeneration>,
}

impl SemanticCandidate {
    #[must_use]
    pub fn from_record(
        record: &crate::ExactRecord,
        score: f32,
        resolution: SemanticResolution,
    ) -> Self {
        Self {
            target: record.target,
            score,
            resolution,
            text: record.text.clone(),
            entity_refs: record.entity_refs.clone(),
            tags: record.tags.clone(),
            current: record.current,
            retired: record.retired,
            purged: record.purged,
            authority_generation: record.authority_generation,
            valid_until: record.valid_until,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticCandidateIndex {
    candidates: Vec<SemanticCandidate>,
}

impl SemanticCandidateIndex {
    #[must_use]
    pub fn new(candidates: Vec<SemanticCandidate>) -> Self {
        Self { candidates }
    }

    #[must_use]
    pub fn candidates(&self) -> &[SemanticCandidate] {
        &self.candidates
    }

    #[must_use]
    pub fn from_vector_hits(
        hits: Vec<VectorHit>,
        exact: &ExactIndex,
        resolution: SemanticResolution,
    ) -> Self {
        let candidates = hits
            .into_iter()
            .filter_map(|hit| {
                let membership = hit.membership();
                let target = CandidateTarget {
                    space_id: membership.space_id(),
                    memory_id: membership.memory_id(),
                    revision_id: membership.revision_id(),
                };
                exact
                    .record_for_target(target)
                    .map(|record| Self::candidate_from_hit(record, hit.score(), resolution))
            })
            .collect();
        Self::new(candidates)
    }

    fn candidate_from_hit(
        record: &crate::ExactRecord,
        score: f32,
        resolution: SemanticResolution,
    ) -> SemanticCandidate {
        SemanticCandidate::from_record(record, score, resolution)
    }
}

pub fn execute_semantic(
    compiled: &CompiledQuery,
    index: &SemanticCandidateIndex,
) -> CandidateResponse {
    if !compiled.execution.used("semantic") {
        return empty_response(compiled);
    }

    let mut candidates = index
        .candidates
        .iter()
        .filter(|candidate| matches_query(compiled, candidate))
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.target.memory_id.cmp(&right.target.memory_id))
            .then_with(|| left.resolution.cmp(&right.resolution))
    });
    candidates.truncate(compiled.query.budget.max_candidates);

    let mut grouped = HashMap::<CandidateTarget, Vec<SemanticCandidate>>::new();
    for candidate in candidates {
        grouped.entry(candidate.target).or_default().push(candidate);
    }

    let mut results = grouped
        .into_values()
        .filter_map(to_evidence)
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        semantic_score(right)
            .total_cmp(&semantic_score(left))
            .then_with(|| left.target.memory_id.cmp(&right.target.memory_id))
            .then_with(|| left.target.revision_id.cmp(&right.target.revision_id))
    });
    results.truncate(compiled.query.budget.max_results);
    CandidateResponse {
        results,
        execution: compiled.execution.clone(),
        snapshot: compiled.snapshot.clone(),
    }
}

fn matches_query(compiled: &CompiledQuery, candidate: &SemanticCandidate) -> bool {
    let query = &compiled.query;
    query.scope.spaces.contains(&candidate.target.space_id)
        && candidate.score.is_finite()
        && candidate.current
        && !candidate.retired
        && !candidate.purged
        && candidate.authority_generation <= compiled.snapshot.authority_generation
        && candidate
            .valid_until
            .is_none_or(|valid_until| compiled.snapshot.authority_generation < valid_until)
        && query
            .constraints
            .memories
            .iter()
            .all(|memory| memory == &candidate.target.memory_id)
        && query
            .constraints
            .entities
            .iter()
            .all(|entity| candidate.entity_refs.contains(entity))
        && query
            .constraints
            .tags
            .iter()
            .all(|tag| candidate.tags.iter().any(|value| value == tag))
}

fn to_evidence(candidates: Vec<SemanticCandidate>) -> Option<CandidateEvidence> {
    let mut by_resolution = BTreeMap::<SemanticResolution, SemanticCandidate>::new();
    for candidate in candidates {
        by_resolution
            .entry(candidate.resolution)
            .and_modify(|existing| {
                if candidate.score > existing.score {
                    *existing = candidate.clone();
                }
            })
            .or_insert(candidate);
    }
    let mut selected = by_resolution.into_values().collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.resolution.priority().cmp(&right.resolution.priority()))
    });
    let primary = selected.first()?.clone();
    Some(CandidateEvidence {
        target: primary.target,
        exact: vec![ExactEvidence {
            field: "revision_id".to_owned(),
            value: primary.target.revision_id.to_string(),
        }],
        lexical: Vec::new(),
        semantic: selected
            .into_iter()
            .map(|candidate| SemanticEvidence {
                score: candidate.score,
                resolution: candidate.resolution,
            })
            .collect(),
        tags: primary
            .tags
            .into_iter()
            .map(|value| TagEvidence { value })
            .collect(),
        propagation: Vec::<PropagationEvidence>::new(),
        relations: Vec::<RelationEvidence>::new(),
        history: Vec::<HistoryEvidence>::new(),
        text: primary.text,
        entity_refs: primary.entity_refs,
    })
}

fn semantic_score(evidence: &CandidateEvidence) -> f32 {
    evidence
        .semantic
        .iter()
        .map(|item| item.score)
        .max_by(f32::total_cmp)
        .unwrap_or(0.0)
}

fn empty_response(compiled: &CompiledQuery) -> CandidateResponse {
    CandidateResponse {
        results: Vec::new(),
        execution: compiled.execution.clone(),
        snapshot: compiled.snapshot.clone(),
    }
}
