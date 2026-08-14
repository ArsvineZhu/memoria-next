use std::collections::BTreeMap;

use memoria_types::{MemoryId, RevisionId, SpaceId};

use crate::compile::CompiledQuery;
use crate::evidence::CandidateEvidence;
use crate::model::{QueryConstraints, QueryScope};
use crate::planner::{CapabilityPlanner, RetrievalProfile};
use crate::snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};

pub type DerivedUnitId = String;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhysicalChannel {
    Exact,
    Lexical,
    SemanticDirect,
    SemanticResidual,
    TagReadout,
    Activation,
    Diffusion,
    Relation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledConstraints {
    pub scope: QueryScope,
    pub constraints: QueryConstraints,
}

impl CompiledConstraints {
    #[must_use]
    pub fn from_query(query: &crate::MemoryQuery) -> Self {
        Self {
            scope: query.scope.clone(),
            constraints: query.constraints.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RerankPolicy {
    pub enabled: bool,
    pub max_candidates: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdaptivePolicy {
    pub enabled: bool,
    pub snapshot: AdaptiveSnapshotIdentity,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhysicalQueryPlan {
    pub snapshot: QuerySnapshot,
    pub profile: RetrievalProfile,
    pub channels: Vec<PhysicalChannel>,
    pub hard_constraints: CompiledConstraints,
    pub rerank: RerankPolicy,
    pub adaptive: AdaptivePolicy,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CandidateKey {
    pub space_id: SpaceId,
    pub memory_id: MemoryId,
    pub revision_id: RevisionId,
    pub derived_unit_id: Option<DerivedUnitId>,
}

impl CandidateKey {
    #[must_use]
    pub const fn new(
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
        derived_unit_id: Option<DerivedUnitId>,
    ) -> Self {
        Self {
            space_id,
            memory_id,
            revision_id,
            derived_unit_id,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandidatePool {
    pub by_key: BTreeMap<CandidateKey, CandidateEvidence>,
    pub channel_ranks: BTreeMap<PhysicalChannel, Vec<CandidateKey>>,
}

impl CandidatePool {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        channel: PhysicalChannel,
        key: CandidateKey,
        evidence: CandidateEvidence,
    ) {
        self.by_key
            .entry(key.clone())
            .and_modify(|existing| merge_evidence(existing, evidence.clone()))
            .or_insert(evidence);
        let ranks = self.channel_ranks.entry(channel).or_default();
        if !ranks.contains(&key) {
            ranks.push(key);
        }
    }

    #[must_use]
    pub fn keys_for_channel(&self, channel: PhysicalChannel) -> &[CandidateKey] {
        self.channel_ranks
            .get(&channel)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

pub struct PhysicalQueryPlanner;

impl PhysicalQueryPlanner {
    #[must_use]
    pub fn plan(compiled: &CompiledQuery) -> PhysicalQueryPlan {
        let query = &compiled.query;
        let execution = &compiled.execution;
        let profile = CapabilityPlanner::retrieval_profile(query);
        let mut channels = Vec::new();

        if query.cue.text.is_empty()
            || !query.cue.memories.is_empty()
            || !query.constraints.memories.is_empty()
            || !query.constraints.entities.is_empty()
            || !query.constraints.tags.is_empty()
            || query.constraints.valid_at.is_some()
            || query.constraints.lifecycle.is_some()
            || !matches!(query.history.mode, crate::QueryHistoryMode::Current)
        {
            channels.push(PhysicalChannel::Exact);
        }
        if !query.cue.text.is_empty() {
            channels.push(PhysicalChannel::Lexical);
        }
        if execution.used("semantic") && !query.cue.text.is_empty() {
            channels.push(PhysicalChannel::SemanticDirect);
            channels.push(PhysicalChannel::SemanticResidual);
        }

        let associative = execution.used("associative");
        let has_tag_seed = !query.cue.tags.is_empty() || execution.used("semantic");
        if associative && has_tag_seed {
            channels.push(PhysicalChannel::TagReadout);
            channels.push(PhysicalChannel::Activation);
            if profile.run_diffusion {
                channels.push(PhysicalChannel::Diffusion);
            }
            channels.push(PhysicalChannel::Relation);
        }

        PhysicalQueryPlan {
            snapshot: compiled.snapshot.clone(),
            profile,
            channels,
            hard_constraints: CompiledConstraints::from_query(query),
            rerank: RerankPolicy {
                enabled: execution.used("reranking"),
                max_candidates: profile.rerank_candidates,
            },
            adaptive: AdaptivePolicy {
                enabled: execution.used("adaptive"),
                snapshot: compiled.snapshot.adaptive.clone(),
            },
        }
    }
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
