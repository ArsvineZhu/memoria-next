use std::collections::{BTreeMap, BTreeSet, HashMap};

use memoria_adaptive::{AdaptiveStateV1, QueryAdaptiveSignature};
use memoria_types::SpaceId;
use memoria_types::Timestamp;

use crate::adaptive::adaptive_prior;
use crate::algorithms::{structure_bonus, support_bonus_from_components};
use crate::evidence::{CandidateEvidence, CandidateTarget, LexicalEvidence, SemanticChannel};
use crate::executor::CandidatePool;
use crate::relation_expand::relation_bonus;

pub const RRF_K: f32 = 60.0;

#[derive(Clone, Debug, PartialEq)]
pub struct FusedCandidate {
    pub evidence: CandidateEvidence,
    pub score: f32,
    pub independent_support_count: usize,
    pub correlation_suppressed_evidence: usize,
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
                        independent_support_count: 0,
                        correlation_suppressed_evidence: 0,
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

/// Fuse the independently ranked lists stored in a physical CandidatePool.
///
/// Physical units that collapse to the same Memory/Revision are counted only
/// once per channel. Their provenance remains in the merged evidence, but a
/// parent/child or multi-resolution fan-out cannot manufacture extra RRF
/// contributions.
pub fn fuse_candidate_pool(
    pool: &CandidatePool,
    limit: usize,
    allowed_spaces: &[SpaceId],
) -> Vec<FusedCandidate> {
    let mut suppressed_by_target = BTreeMap::<CandidateTarget, usize>::new();
    let channels = pool
        .channel_ranks
        .values()
        .map(|keys| {
            let mut by_target = BTreeMap::<CandidateTarget, CandidateEvidence>::new();
            let mut order = Vec::new();
            for key in keys {
                let Some(evidence) = pool.by_key.get(key).cloned() else {
                    continue;
                };
                let target = evidence.target;
                if let Some(existing) = by_target.get_mut(&target) {
                    merge_evidence(existing, evidence);
                    *suppressed_by_target.entry(target).or_default() += 1;
                } else {
                    by_target.insert(target, evidence);
                    order.push(target);
                }
            }
            order
                .into_iter()
                .filter_map(|target| by_target.remove(&target))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let mut fused = fuse_channels_scoped(channels, RRF_K, limit, allowed_spaces);
    for candidate in &mut fused {
        let (independent, correlated) = correlation_counts(&candidate.evidence);
        candidate.independent_support_count = independent;
        candidate.correlation_suppressed_evidence = correlated
            + suppressed_by_target
                .get(&candidate.evidence.target)
                .copied()
                .unwrap_or_default();
        let support = propagation_support_bonus(&candidate.evidence);
        let structure = semantic_structure_bonus(&candidate.evidence);
        let relation = relation_bonus(candidate.evidence.relations.len());
        candidate.score += support + structure + relation;
    }
    fused.sort_by(|left, right| {
        right.score.total_cmp(&left.score).then_with(|| {
            left.evidence
                .target
                .memory_id
                .cmp(&right.evidence.target.memory_id)
        })
    });
    fused
}

fn correlation_counts(evidence: &CandidateEvidence) -> (usize, usize) {
    let mut independent = BTreeSet::new();
    if !evidence.exact.is_empty() {
        independent.insert("exact");
    }
    if !evidence.lexical.is_empty() {
        independent.insert("lexical");
    }
    let semantic_channels = evidence
        .semantic
        .iter()
        .map(|item| match item.channel {
            SemanticChannel::Direct => "semantic-direct",
            SemanticChannel::Residual => "semantic-residual",
        })
        .collect::<BTreeSet<_>>();
    if !semantic_channels.is_empty() {
        independent.extend(semantic_channels.iter().copied());
    }
    if !evidence.tags.is_empty() {
        independent.insert("tag");
    }
    if !evidence.propagation.is_empty() {
        independent.insert("propagation");
    }
    if !evidence.relations.is_empty() {
        independent.insert("relation");
    }
    if !evidence.history.is_empty() {
        independent.insert("history");
    }
    (
        independent.len(),
        evidence
            .semantic
            .len()
            .saturating_sub(semantic_channels.len()),
    )
}

fn propagation_support_bonus(evidence: &CandidateEvidence) -> f32 {
    let Some(strongest) = evidence
        .propagation
        .iter()
        .map(|item| item.score)
        .max_by(f32::total_cmp)
    else {
        return 0.0;
    };
    support_bonus_from_components(evidence.propagation.len(), strongest, 0.0)
}

fn semantic_structure_bonus(evidence: &CandidateEvidence) -> f32 {
    let resolutions = evidence
        .semantic
        .iter()
        .map(|item| item.resolution)
        .collect::<Vec<_>>();
    if resolutions.len() < 2 {
        return 0.0;
    }
    let unique = resolutions.iter().copied().collect::<BTreeSet<_>>().len();
    structure_bonus((unique as f32 / 3.0).clamp(0.0, 1.0), 0, unique > 1)
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
    fuse_channels(channels, k, usize::MAX)
        .into_iter()
        .filter(|candidate| scope.contains(&candidate.evidence.target.space_id))
        .take(limit)
        .collect()
}

pub fn fuse_channels_with_adaptive(
    channels: impl IntoIterator<Item = Vec<CandidateEvidence>>,
    k: f32,
    limit: usize,
    allowed_spaces: &[SpaceId],
    state: &AdaptiveStateV1,
    signature: &QueryAdaptiveSignature,
    now: Timestamp,
) -> Vec<FusedCandidate> {
    let mut fused = fuse_channels_scoped(channels, k, limit, allowed_spaces);
    fused.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| {
                let left_prior = adaptive_prior(
                    state,
                    signature,
                    left.evidence.target.space_id,
                    left.evidence.target.memory_id,
                    now,
                );
                let right_prior = adaptive_prior(
                    state,
                    signature,
                    right.evidence.target.space_id,
                    right.evidence.target.memory_id,
                    now,
                );
                right_prior.total_cmp(&left_prior)
            })
            .then_with(|| {
                left.evidence
                    .target
                    .memory_id
                    .cmp(&right.evidence.target.memory_id)
            })
    });
    fused
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
