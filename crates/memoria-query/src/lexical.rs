use memoria_derived::LexicalHit;

use crate::EntityRef;
use crate::compile::CompiledQuery;
use crate::evidence::{
    CandidateEvidence, CandidateResponse, CandidateTarget, ExactEvidence, HistoryEvidence,
    LexicalEvidence, PropagationEvidence, RelationEvidence, SemanticEvidence, TagEvidence,
};
use crate::exact::ExactIndex;

#[derive(Clone, Debug, PartialEq)]
pub struct LexicalCandidate {
    pub target: CandidateTarget,
    pub score: f32,
    pub text: String,
    pub entity_refs: Vec<EntityRef>,
    pub tags: Vec<String>,
    pub current: bool,
    pub retired: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LexicalCandidateIndex {
    candidates: Vec<LexicalCandidate>,
}

impl LexicalCandidateIndex {
    #[must_use]
    pub fn new(candidates: Vec<LexicalCandidate>) -> Self {
        Self { candidates }
    }

    #[must_use]
    pub fn candidates(&self) -> &[LexicalCandidate] {
        &self.candidates
    }

    #[must_use]
    pub fn from_derived_hits(hits: Vec<LexicalHit>, exact: &ExactIndex) -> Self {
        let candidates = hits
            .into_iter()
            .filter_map(|hit| {
                let target = CandidateTarget {
                    space_id: hit.space_id,
                    memory_id: hit.memory_id,
                    revision_id: hit.revision_id,
                };
                let record = exact.record_for_target(target)?;
                Some(LexicalCandidate {
                    target,
                    score: hit.score,
                    text: record.text.clone(),
                    entity_refs: record.entity_refs.clone(),
                    tags: record.tags.clone(),
                    current: record.current,
                    retired: record.retired,
                })
            })
            .collect();
        Self::new(candidates)
    }
}

pub fn execute_lexical(
    compiled: &CompiledQuery,
    index: &LexicalCandidateIndex,
) -> CandidateResponse {
    let mut candidates = index.candidates.clone();
    candidates.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.target.memory_id.cmp(&right.target.memory_id))
    });
    let results = candidates
        .into_iter()
        .enumerate()
        .filter(|(_, candidate)| matches_query(compiled, candidate))
        .map(|(rank, candidate)| to_evidence(rank, candidate))
        .take(compiled.query.budget.max_results)
        .collect();
    CandidateResponse {
        results,
        execution: compiled.execution.clone(),
        snapshot: compiled.snapshot.clone(),
    }
}

fn matches_query(compiled: &CompiledQuery, candidate: &LexicalCandidate) -> bool {
    let query = &compiled.query;
    query.scope.spaces.contains(&candidate.target.space_id)
        && candidate.current
        && !candidate.retired
        && query.cue.text.iter().any(|cue| {
            candidate
                .text
                .to_ascii_lowercase()
                .contains(&cue.to_ascii_lowercase())
        })
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

fn to_evidence(rank: usize, candidate: LexicalCandidate) -> CandidateEvidence {
    CandidateEvidence {
        target: candidate.target,
        exact: vec![ExactEvidence {
            field: "revision_id".to_owned(),
            value: candidate.target.revision_id.to_string(),
        }],
        lexical: vec![LexicalEvidence {
            rank,
            score: candidate.score,
        }],
        semantic: Vec::<SemanticEvidence>::new(),
        tags: candidate
            .tags
            .into_iter()
            .map(|value| TagEvidence { value })
            .collect(),
        propagation: Vec::<PropagationEvidence>::new(),
        relations: Vec::<RelationEvidence>::new(),
        history: Vec::<HistoryEvidence>::new(),
        text: candidate.text,
        entity_refs: candidate.entity_refs,
    }
}
