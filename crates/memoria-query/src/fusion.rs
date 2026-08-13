use std::collections::HashMap;

use memoria_types::SpaceId;

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

pub fn fuse_channels_scoped(
    channels: impl IntoIterator<Item = Vec<CandidateEvidence>>,
    k: f32,
    limit: usize,
    allowed_spaces: &[SpaceId],
) -> Vec<FusedCandidate> {
    let scope = allowed_spaces
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    fuse_channels(channels, k, limit)
        .into_iter()
        .filter(|candidate| scope.contains(&candidate.evidence.target.space_id))
        .collect()
}

fn merge_evidence(target: &mut CandidateEvidence, source: CandidateEvidence) {
    extend_unique(&mut target.exact, source.exact);
    extend_unique(&mut target.lexical, source.lexical);
    extend_unique(&mut target.semantic, source.semantic);
    extend_unique(&mut target.tags, source.tags);
    extend_unique(&mut target.propagation, source.propagation);
    extend_unique(&mut target.relations, source.relations);
    extend_unique(&mut target.history, source.history);
    if target.text.is_empty() {
        target.text = source.text;
    }
    for entity in source.entity_refs {
        if !target.entity_refs.contains(&entity) {
            target.entity_refs.push(entity);
        }
    }
}

fn extend_unique<T: PartialEq>(target: &mut Vec<T>, source: impl IntoIterator<Item = T>) {
    for item in source {
        if !target.contains(&item) {
            target.push(item);
        }
    }
}

#[allow(dead_code)]
fn _lexical_ranks_are_preserved(
    evidence: &CandidateEvidence,
) -> impl Iterator<Item = &LexicalEvidence> {
    evidence.lexical.iter()
}
