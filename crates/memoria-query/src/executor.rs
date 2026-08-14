use std::collections::BTreeMap;

use memoria_derived::{TagDictionary, TagGraph};
use memoria_types::{MemoryId, RevisionId, SpaceId};

use crate::algorithms::{
    activation_propagate, diffusion_propagate, l2_norm, project_tag_basis_with_limit,
};
use crate::compile::CompiledQuery;
use crate::evidence::{
    CandidateEvidence, CandidateResponse, ExactEvidence, HistoryEvidence, PropagationEvidence,
    RelationEvidence, SemanticEvidence, TagEvidence,
};
use crate::exact::{ExactIndex, ExactRecord, matches_query};
use crate::model::{QueryConstraints, QueryScope};
use crate::planner::{CapabilityPlanner, RetrievalProfile};
use crate::relation_expand::{RelationLink, expand_relations};
use crate::snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};
use crate::tags::{
    CompositeTagView, TagReadoutCandidate, TagSeed, TagVectorCandidate, readout_tag_candidates,
    select_tag_basis_candidates,
};
use crate::trace::QueryOperatorTrace;
use crate::validate::QueryError;

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

    pub fn insert_channel(
        &mut self,
        channel: PhysicalChannel,
        candidates: impl IntoIterator<Item = CandidateEvidence>,
    ) {
        for evidence in candidates {
            let target = evidence.target;
            self.insert(
                channel,
                CandidateKey::new(target.space_id, target.memory_id, target.revision_id, None),
                evidence,
            );
        }
    }

    pub fn insert_response(&mut self, channel: PhysicalChannel, response: CandidateResponse) {
        self.insert_channel(channel, response.results);
    }

    #[must_use]
    pub fn candidates(&self) -> Vec<CandidateEvidence> {
        self.by_key.values().cloned().collect()
    }

    #[must_use]
    pub fn keys_for_channel(&self, channel: PhysicalChannel) -> &[CandidateKey] {
        self.channel_ranks
            .get(&channel)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[derive(Clone, Debug, Default)]
pub struct AlgorithmChannelInputs {
    pub tag_dictionary: TagDictionary,
    pub tag_graph: TagGraph,
    pub tag_vectors: Vec<TagVectorCandidate>,
    pub tag_seeds: Vec<TagSeed>,
    pub relation_links: Vec<RelationLink>,
}

pub trait SemanticResidualOperator {
    fn search(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<CandidateEvidence>, QueryError>;
}

pub fn execute_algorithm_channels(
    compiled: &CompiledQuery,
    query_vector: &[f32],
    exact: &ExactIndex,
    pool: &mut CandidatePool,
    inputs: &AlgorithmChannelInputs,
    residual_operator: Option<&dyn SemanticResidualOperator>,
) -> Result<QueryOperatorTrace, QueryError> {
    let plan = PhysicalQueryPlanner::plan(compiled);
    let mut trace = QueryOperatorTrace {
        authority_generation: compiled.snapshot.authority_generation,
        ..QueryOperatorTrace::default()
    };

    let semantic_active = compiled.execution.used("semantic");
    let selected_tag_vectors = if semantic_active {
        select_tag_basis_candidates(
            inputs.tag_vectors.clone(),
            true,
            plan.profile.tag_basis_vectors,
        )
    } else {
        Vec::new()
    };

    let tag_basis = if semantic_active && !query_vector.is_empty() {
        if selected_tag_vectors.is_empty() {
            None
        } else {
            let vectors = selected_tag_vectors
                .iter()
                .map(|candidate| candidate.vector.clone())
                .collect::<Vec<_>>();
            let basis = project_tag_basis_with_limit(
                query_vector,
                &vectors,
                plan.profile.tag_basis_vectors,
            )
            .map_err(|error| QueryError::OperatorFailure {
                message: error.to_string(),
            })?;
            trace.record_tag_basis(basis.rank, basis.conditioning, basis.explained_energy);
            Some(basis)
        }
    } else {
        None
    };

    if let Some(basis) = &tag_basis
        && basis.used
        && l2_norm(&basis.residual) > 1.0e-4
        && plan.channels.contains(&PhysicalChannel::SemanticResidual)
        && let Some(operator) = residual_operator
    {
        pool.insert_channel(
            PhysicalChannel::SemanticResidual,
            operator.search(&basis.residual, plan.profile.tag_basis_vectors.max(1))?,
        );
        record_channel(
            &mut trace,
            pool,
            PhysicalChannel::SemanticResidual,
            "semantic-residual",
        );
    }

    let mut seeds = inputs.tag_seeds.clone();
    for candidate in &selected_tag_vectors {
        merge_seed(
            &mut seeds,
            TagSeed {
                tag_id: candidate.tag_id,
                value: inputs
                    .tag_dictionary
                    .value(candidate.tag_id)
                    .unwrap_or("unknown")
                    .to_owned(),
                provenance: candidate.provenance,
            },
        );
    }

    if plan.channels.contains(&PhysicalChannel::TagReadout) && !seeds.is_empty() {
        let seed_ids = seeds.iter().map(|seed| seed.tag_id).collect::<Vec<_>>();
        let candidates =
            readout_tag_candidates(&inputs.tag_graph, &seed_ids, &compiled.query.scope.spaces)
                .into_iter()
                .filter_map(|candidate| tag_readout_evidence(exact, compiled, inputs, candidate))
                .collect::<Vec<_>>();
        pool.insert_channel(PhysicalChannel::TagReadout, candidates);
        record_channel(&mut trace, pool, PhysicalChannel::TagReadout, "tag-readout");
    }

    if plan.channels.contains(&PhysicalChannel::Activation) && !seeds.is_empty() {
        let view = CompositeTagView::from_scope(
            &inputs.tag_graph,
            compiled.query.scope.spaces.iter().copied(),
        );
        let graph = crate::AssociationGraph::from_composite(&view).map_err(|error| {
            QueryError::OperatorFailure {
                message: error.to_string(),
            }
        })?;
        let activation = activation_propagate(&graph, &seeds, plan.profile.activation_budget)
            .map_err(|error| QueryError::OperatorFailure {
                message: error.to_string(),
            })?;
        trace.activation_edge_visits = activation.edge_visits;
        trace.activation_hops = activation.max_hops_reached;
        trace.activation_truncated = activation.truncated;
        let activation_candidates =
            propagated_tag_evidence(exact, compiled, inputs, &activation.active_tags);
        pool.insert_channel(PhysicalChannel::Activation, activation_candidates);
        record_channel(&mut trace, pool, PhysicalChannel::Activation, "activation");

        if plan.channels.contains(&PhysicalChannel::Diffusion) {
            let diffusion = diffusion_propagate(&graph, &seeds, plan.profile.activation_budget)
                .map_err(|error| QueryError::OperatorFailure {
                    message: error.to_string(),
                })?;
            trace.diffusion_iterations = diffusion.iterations;
            trace.diffusion_convergence_delta = Some(diffusion.convergence_delta);
            trace.diffusion_truncated = diffusion.truncated;
            let diffusion_candidates =
                propagated_tag_evidence(exact, compiled, inputs, &diffusion.active_tags);
            pool.insert_channel(PhysicalChannel::Diffusion, diffusion_candidates);
            record_channel(&mut trace, pool, PhysicalChannel::Diffusion, "diffusion");
        }
    }

    if plan.channels.contains(&PhysicalChannel::Relation) && !inputs.relation_links.is_empty() {
        let expanded = expand_relations(
            pool.candidates(),
            &inputs.relation_links,
            &compiled.query.scope.spaces,
            plan.profile.relation_budget,
        )
        .map_err(|error| QueryError::OperatorFailure {
            message: error.to_string(),
        })?;
        let relation_candidates = expanded
            .into_iter()
            .filter(|candidate| !candidate.relations.is_empty())
            .filter(|candidate| {
                exact
                    .record_for_target(candidate.target)
                    .is_some_and(|record| matches_query(compiled, record))
            })
            .collect::<Vec<_>>();
        trace.relation_expansions = relation_candidates.len();
        pool.insert_channel(PhysicalChannel::Relation, relation_candidates);
        record_channel(&mut trace, pool, PhysicalChannel::Relation, "relation");
    }

    if tag_basis.is_none() {
        trace.tag_basis_rank = None;
    }
    Ok(trace)
}

fn record_channel(
    trace: &mut QueryOperatorTrace,
    pool: &CandidatePool,
    channel: PhysicalChannel,
    name: &str,
) {
    *trace = std::mem::take(trace).with_channel(name, pool.keys_for_channel(channel).len());
}

fn merge_seed(seeds: &mut Vec<TagSeed>, candidate: TagSeed) {
    if let Some(existing) = seeds
        .iter_mut()
        .find(|seed| seed.tag_id == candidate.tag_id)
    {
        if candidate.provenance.weight() > existing.provenance.weight() {
            *existing = candidate;
        }
    } else {
        seeds.push(candidate);
    }
}

fn tag_readout_evidence(
    exact: &ExactIndex,
    compiled: &CompiledQuery,
    inputs: &AlgorithmChannelInputs,
    candidate: TagReadoutCandidate,
) -> Option<CandidateEvidence> {
    let record = exact.record_for_target(candidate.target)?;
    if !matches_query(compiled, record) {
        return None;
    }
    let mut evidence = record_evidence(record);
    let value = inputs
        .tag_dictionary
        .value(candidate.tag_id)
        .unwrap_or("unknown")
        .to_owned();
    if !evidence.tags.iter().any(|tag| tag.value == value) {
        evidence.tags.push(TagEvidence { value });
    }
    Some(evidence)
}

fn propagated_tag_evidence(
    exact: &ExactIndex,
    compiled: &CompiledQuery,
    inputs: &AlgorithmChannelInputs,
    propagated: &[crate::PropagatedTag],
) -> Vec<CandidateEvidence> {
    let scores = propagated
        .iter()
        .map(|tag| (tag.tag_id, tag.score))
        .collect::<BTreeMap<_, _>>();
    let tag_ids = scores.keys().copied().collect::<Vec<_>>();
    readout_tag_candidates(&inputs.tag_graph, &tag_ids, &compiled.query.scope.spaces)
        .into_iter()
        .filter_map(|candidate| {
            let record = exact.record_for_target(candidate.target)?;
            if !matches_query(compiled, record) {
                return None;
            }
            let mut evidence = record_evidence(record);
            let value = inputs
                .tag_dictionary
                .value(candidate.tag_id)
                .unwrap_or("unknown")
                .to_owned();
            if !evidence.tags.iter().any(|tag| tag.value == value) {
                evidence.tags.push(TagEvidence { value });
            }
            evidence.propagation.push(PropagationEvidence {
                score: scores.get(&candidate.tag_id).copied().unwrap_or(0.0) * candidate.score,
            });
            Some(evidence)
        })
        .collect()
}

fn record_evidence(record: &ExactRecord) -> CandidateEvidence {
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
        propagation: Vec::new(),
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
