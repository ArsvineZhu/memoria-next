use std::collections::{BTreeSet, HashMap};

use thiserror::Error;

use crate::evidence::{CandidateEvidence, CandidateTarget};
use crate::fusion::FusedCandidate;
use crate::model::QueryBudget;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ConsolidationError {
    #[error("consolidation result budget must be greater than zero")]
    InvalidBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsolidationCandidate {
    pub evidence: CandidateEvidence,
    pub score: f32,
}

impl ConsolidationCandidate {
    #[must_use]
    pub const fn new(evidence: CandidateEvidence, score: f32) -> Self {
        Self { evidence, score }
    }
}

impl From<CandidateEvidence> for ConsolidationCandidate {
    fn from(evidence: CandidateEvidence) -> Self {
        let score = evidence_strength(&evidence);
        Self { evidence, score }
    }
}

impl From<FusedCandidate> for ConsolidationCandidate {
    fn from(candidate: FusedCandidate) -> Self {
        Self {
            evidence: candidate.evidence,
            score: candidate.score,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryMatch {
    pub evidence: CandidateEvidence,
    pub score: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryResult {
    pub space_id: memoria_types::SpaceId,
    pub memory_id: memoria_types::MemoryId,
    pub revision_id: memoria_types::RevisionId,
    pub matches: Vec<MemoryMatch>,
    pub relevance: f32,
    pub confidence: f32,
    pub accessibility: f32,
    pub effort: f32,
}

pub fn consolidate<I, C>(
    candidates: I,
    budget: QueryBudget,
) -> Result<Vec<MemoryResult>, ConsolidationError>
where
    I: IntoIterator<Item = C>,
    C: Into<ConsolidationCandidate>,
{
    if budget.max_results == 0 {
        return Err(ConsolidationError::InvalidBudget);
    }
    let mut groups = HashMap::<CandidateTarget, Vec<ConsolidationCandidate>>::new();
    for candidate in candidates {
        let candidate = candidate.into();
        groups
            .entry(candidate.evidence.target)
            .or_default()
            .push(candidate);
    }
    let mut results = groups
        .into_iter()
        .map(|(target, candidates)| consolidate_group(target, candidates, budget))
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .relevance
            .total_cmp(&left.relevance)
            .then_with(|| left.memory_id.cmp(&right.memory_id))
            .then_with(|| left.revision_id.cmp(&right.revision_id))
    });
    results.truncate(budget.max_results);
    Ok(results)
}

fn consolidate_group(
    target: CandidateTarget,
    mut candidates: Vec<ConsolidationCandidate>,
    budget: QueryBudget,
) -> MemoryResult {
    candidates.sort_by(|left, right| right.score.total_cmp(&left.score));
    let primary = candidates.first().map_or(0.0, |candidate| candidate.score);
    let mut independent_families = candidates
        .first()
        .map(|candidate| evidence_families(&candidate.evidence))
        .unwrap_or_default();
    let mut support = 0.0;
    for candidate in candidates.iter().skip(1) {
        let families = evidence_families(&candidate.evidence);
        if families
            .iter()
            .any(|family| !independent_families.contains(family))
        {
            support += candidate.score.max(0.0);
            independent_families.extend(families);
        }
    }
    let score = primary + 0.5 * (1.0 - (-support).exp());
    let mut matches = Vec::with_capacity(1);
    let mut merged = candidates.remove(0).evidence;
    for candidate in candidates {
        merge_evidence(&mut merged, candidate.evidence);
    }
    trim_evidence(&mut merged, budget.max_evidence_tokens);
    matches.push(MemoryMatch {
        evidence: merged,
        score,
    });
    let evidence = &matches[0].evidence;
    let evidence_channels = [
        !evidence.exact.is_empty(),
        !evidence.lexical.is_empty(),
        !evidence.semantic.is_empty(),
        !evidence.tags.is_empty(),
        !evidence.propagation.is_empty(),
        !evidence.relations.is_empty(),
        !evidence.history.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    MemoryResult {
        space_id: target.space_id,
        memory_id: target.memory_id,
        revision_id: target.revision_id,
        matches,
        relevance: score / (1.0 + score.max(0.0)),
        confidence: (evidence_channels as f32 / 3.0).min(1.0),
        accessibility: 0.5,
        effort: 1.0 / (1.0 + evidence_channels as f32),
    }
}

fn evidence_strength(evidence: &CandidateEvidence) -> f32 {
    evidence.exact.len() as f32
        + evidence.lexical.len() as f32 * 0.5
        + evidence.semantic.len() as f32 * 0.5
        + evidence.tags.len() as f32 * 0.25
        + evidence.relations.len() as f32 * 0.25
        + evidence.history.len() as f32 * 0.25
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

fn evidence_families(evidence: &CandidateEvidence) -> BTreeSet<&'static str> {
    let mut families = BTreeSet::new();
    if !evidence.exact.is_empty() {
        families.insert("exact");
    }
    if !evidence.lexical.is_empty() {
        families.insert("lexical");
    }
    for semantic in &evidence.semantic {
        families.insert(match semantic.channel {
            crate::SemanticChannel::Direct => "semantic-direct",
            crate::SemanticChannel::Residual => "semantic-residual",
        });
    }
    if !evidence.tags.is_empty() {
        families.insert("tags");
    }
    if !evidence.propagation.is_empty() {
        families.insert("propagation");
    }
    if !evidence.relations.is_empty() {
        families.insert("relations");
    }
    if !evidence.history.is_empty() {
        families.insert("history");
    }
    families
}

fn trim_evidence(evidence: &mut CandidateEvidence, max_entries: usize) {
    let mut remaining = max_entries;
    trim_vec(&mut evidence.exact, &mut remaining);
    trim_vec(&mut evidence.lexical, &mut remaining);
    trim_vec(&mut evidence.semantic, &mut remaining);
    trim_vec(&mut evidence.tags, &mut remaining);
    trim_vec(&mut evidence.propagation, &mut remaining);
    trim_vec(&mut evidence.relations, &mut remaining);
    trim_vec(&mut evidence.history, &mut remaining);
    if remaining == 0 {
        evidence.entity_refs.clear();
    }
}

fn trim_vec<T>(values: &mut Vec<T>, remaining: &mut usize) {
    if *remaining >= values.len() {
        *remaining -= values.len();
    } else {
        values.truncate(*remaining);
        *remaining = 0;
    }
}

fn extend_unique<T: PartialEq>(target: &mut Vec<T>, source: impl IntoIterator<Item = T>) {
    for item in source {
        if !target.contains(&item) {
            target.push(item);
        }
    }
}
