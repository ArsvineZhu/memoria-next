use std::collections::HashMap;

use crate::evidence::{CandidateEvidence, CandidateTarget, LexicalEvidence};

#[derive(Clone, Debug, PartialEq)]
pub struct FusedCandidate {
    pub evidence: CandidateEvidence,
    pub score: f32,
}

#[must_use]
pub fn rrf(rank: usize, k: f32) -> f32 {
    1.0 / (k + rank as f32)
}

pub fn fuse_channels(
    channels: impl IntoIterator<Item = Vec<CandidateEvidence>>,
    k: f32,
    limit: usize,
) -> Vec<FusedCandidate> {
    let mut merged = HashMap::<CandidateTarget, FusedCandidate>::new();
    for channel in channels {
        for (rank, evidence) in channel.into_iter().enumerate() {
            let target = evidence.target;
            let contribution = rrf(rank, k);
            if let Some(existing) = merged.get_mut(&target) {
                merge_evidence(&mut existing.evidence, evidence);
                existing.score += contribution;
            } else {
                merged.insert(
                    target,
                    FusedCandidate {
                        evidence,
                        score: contribution,
                    },
                );
            }
        }
    }
    let mut values = merged.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right.score.total_cmp(&left.score).then_with(|| {
            left.evidence
                .target
                .memory_id
                .cmp(&right.evidence.target.memory_id)
        })
    });
    values.truncate(limit);
    values
}

fn merge_evidence(target: &mut CandidateEvidence, source: CandidateEvidence) {
    target.exact.extend(source.exact);
    target.lexical.extend(source.lexical);
    target.semantic.extend(source.semantic);
    target.tags.extend(source.tags);
    target.propagation.extend(source.propagation);
    target.relations.extend(source.relations);
    target.history.extend(source.history);
    if target.text.is_empty() {
        target.text = source.text;
    }
    for entity in source.entity_refs {
        if !target.entity_refs.contains(&entity) {
            target.entity_refs.push(entity);
        }
    }
}

#[allow(dead_code)]
fn _lexical_ranks_are_preserved(
    evidence: &CandidateEvidence,
) -> impl Iterator<Item = &LexicalEvidence> {
    evidence.lexical.iter()
}
