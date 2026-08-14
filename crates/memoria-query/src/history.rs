use memoria_types::AuthorityGeneration;

use crate::compile::CompiledQuery;
use crate::evidence::{
    CandidateEvidence, CandidateResponse, ExactEvidence, HistoryEvidence, PropagationEvidence,
    RelationEvidence, SemanticEvidence, TagEvidence,
};
use crate::exact::{ExactIndex, ExactRecord, visible_at};

pub fn execute_history(compiled: &CompiledQuery, index: &ExactIndex) -> CandidateResponse {
    let mut records = index
        .records()
        .iter()
        .filter(|record| matches_query(compiled, record))
        .collect::<Vec<_>>();
    records.sort_by(|left, right| {
        right
            .authority_generation
            .cmp(&left.authority_generation)
            .then_with(|| left.target.memory_id.cmp(&right.target.memory_id))
            .then_with(|| left.target.revision_id.cmp(&right.target.revision_id))
    });
    let results = records
        .into_iter()
        .take(compiled.query.budget.max_results)
        .map(to_evidence)
        .collect();
    CandidateResponse {
        results,
        execution: compiled.execution.clone(),
        snapshot: compiled.snapshot.clone(),
    }
}

fn matches_query(compiled: &CompiledQuery, record: &ExactRecord) -> bool {
    let query = &compiled.query;
    query.scope.spaces.contains(&record.target.space_id)
        && !record.purged
        && created_at_or_before(record, compiled.snapshot.authority_generation)
        && text_matches(record, &query.cue.text)
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

fn created_at_or_before(record: &ExactRecord, generation: AuthorityGeneration) -> bool {
    visible_at(record, generation) || record.authority_generation <= generation
}

fn text_matches(record: &ExactRecord, cues: &[String]) -> bool {
    if cues.is_empty() {
        return true;
    }
    let text = record.text.to_ascii_lowercase();
    cues.iter()
        .any(|cue| text.contains(&cue.to_ascii_lowercase()))
}

fn to_evidence(record: &ExactRecord) -> CandidateEvidence {
    CandidateEvidence {
        target: record.target,
        exact: vec![
            ExactEvidence {
                field: "memory_id".to_owned(),
                value: record.target.memory_id.to_string(),
            },
            ExactEvidence {
                field: "revision_id".to_owned(),
                value: record.target.revision_id.to_string(),
            },
        ],
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
        history: vec![HistoryEvidence {
            detail: format!(
                "retained revision at authority generation {}",
                record.authority_generation
            ),
        }],
        text: record.text.clone(),
        entity_refs: record.entity_refs.clone(),
    }
}
