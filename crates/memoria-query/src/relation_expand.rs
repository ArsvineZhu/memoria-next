use std::collections::{HashMap, HashSet};

use memoria_types::SpaceId;
use thiserror::Error;

use crate::evidence::{CandidateEvidence, CandidateTarget, RelationEvidence};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelationExpansionBudget {
    pub max_hops: usize,
    pub max_added: usize,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum RelationExpansionError {
    #[error("relation expansion budget values must be greater than zero")]
    InvalidBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RelationLink {
    pub source: CandidateTarget,
    pub target: CandidateTarget,
    pub relation: String,
    pub candidate: CandidateEvidence,
    authoritative: bool,
}

impl RelationLink {
    #[must_use]
    pub fn new(
        source: CandidateTarget,
        target: CandidateTarget,
        relation: impl Into<String>,
        candidate: CandidateEvidence,
    ) -> Self {
        Self {
            source,
            target,
            relation: relation.into(),
            candidate,
            authoritative: true,
        }
    }

    /// Construct an association-graph edge. It is intentionally not eligible
    /// for explicit Relation expansion or the Relation ranking bonus.
    #[must_use]
    pub fn new_association(
        source: CandidateTarget,
        target: CandidateTarget,
        relation: impl Into<String>,
        candidate: CandidateEvidence,
    ) -> Self {
        Self {
            source,
            target,
            relation: relation.into(),
            candidate,
            authoritative: false,
        }
    }
}

/// Expand only through explicit relation links whose source and target remain
/// inside the query's already-authorized Space scope.
pub fn expand_relations(
    candidates: impl IntoIterator<Item = CandidateEvidence>,
    links: &[RelationLink],
    allowed_spaces: &[SpaceId],
    budget: RelationExpansionBudget,
) -> Result<Vec<CandidateEvidence>, RelationExpansionError> {
    if budget.max_hops == 0 || budget.max_added == 0 {
        return Err(RelationExpansionError::InvalidBudget);
    }
    let scope = allowed_spaces.iter().copied().collect::<HashSet<_>>();
    let mut by_target = HashMap::<CandidateTarget, CandidateEvidence>::new();
    for candidate in candidates {
        if scope.contains(&candidate.target.space_id) {
            by_target
                .entry(candidate.target)
                .and_modify(|existing| merge_evidence(existing, candidate.clone()))
                .or_insert(candidate);
        }
    }

    let mut frontier = by_target.keys().copied().collect::<HashSet<_>>();
    let mut added = 0;
    for _ in 0..budget.max_hops {
        let mut next_frontier = HashSet::new();
        for link in links {
            if !frontier.contains(&link.source)
                || !link.authoritative
                || !scope.contains(&link.target.space_id)
                || link.candidate.target != link.target
                || link.relation.trim().is_empty()
            {
                continue;
            }
            if !by_target.contains_key(&link.target) {
                if added >= budget.max_added {
                    break;
                }
                added += 1;
            }
            let mut candidate = link.candidate.clone();
            candidate.relations.push(RelationEvidence {
                relation: link.relation.clone(),
            });
            if let Some(existing) = by_target.get_mut(&link.target) {
                merge_evidence(existing, candidate);
            } else {
                by_target.insert(link.target, candidate);
            }
            next_frontier.insert(link.target);
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }

    let mut output = by_target.into_values().collect::<Vec<_>>();
    output.sort_by(|left, right| compare_targets(left.target, right.target));
    Ok(output)
}

/// Bounded bonus from independent authoritative Relation evidence.
#[must_use]
pub fn relation_bonus(independent_authoritative_relation_count: usize) -> f32 {
    (0.025 * independent_authoritative_relation_count as f32).min(0.05)
}

fn merge_evidence(target: &mut CandidateEvidence, source: CandidateEvidence) {
    for evidence in source.exact {
        if !target.exact.contains(&evidence) {
            target.exact.push(evidence);
        }
    }
    for evidence in source.lexical {
        if !target.lexical.contains(&evidence) {
            target.lexical.push(evidence);
        }
    }
    for evidence in source.semantic {
        if !target.semantic.contains(&evidence) {
            target.semantic.push(evidence);
        }
    }
    for evidence in source.tags {
        if !target.tags.contains(&evidence) {
            target.tags.push(evidence);
        }
    }
    for evidence in source.propagation {
        if !target.propagation.contains(&evidence) {
            target.propagation.push(evidence);
        }
    }
    for evidence in source.relations {
        if !target.relations.contains(&evidence) {
            target.relations.push(evidence);
        }
    }
    for evidence in source.history {
        if !target.history.contains(&evidence) {
            target.history.push(evidence);
        }
    }
    if target.text.is_empty() {
        target.text = source.text;
    }
    for entity in source.entity_refs {
        if !target.entity_refs.contains(&entity) {
            target.entity_refs.push(entity);
        }
    }
}

fn compare_targets(left: CandidateTarget, right: CandidateTarget) -> std::cmp::Ordering {
    left.space_id
        .cmp(&right.space_id)
        .then_with(|| left.memory_id.cmp(&right.memory_id))
        .then_with(|| left.revision_id.cmp(&right.revision_id))
}
