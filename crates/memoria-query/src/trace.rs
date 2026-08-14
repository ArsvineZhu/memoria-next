use memoria_types::AuthorityGeneration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct QueryOperatorTrace {
    pub channels_executed: Vec<String>,
    pub candidate_counts: Vec<(String, usize)>,
    pub tag_basis_rank: Option<usize>,
    pub tag_basis_conditioning: Option<f32>,
    pub tag_basis_explained_energy: Option<f32>,
    pub activation_edge_visits: usize,
    pub activation_hops: usize,
    pub activation_truncated: bool,
    pub diffusion_iterations: usize,
    pub diffusion_convergence_delta: Option<f32>,
    pub diffusion_truncated: bool,
    pub independent_support_count: usize,
    pub correlation_suppressed_evidence: usize,
    pub relation_expansions: usize,
    pub rerank_requested: bool,
    pub rerank_applied: bool,
    pub capability_degraded: bool,
    pub authority_generation: AuthorityGeneration,
}

impl QueryOperatorTrace {
    #[must_use]
    pub fn with_channel(mut self, channel: impl Into<String>, candidate_count: usize) -> Self {
        self.channels_executed.push(channel.into());
        self.candidate_counts.push((
            self.channels_executed.last().cloned().unwrap_or_default(),
            candidate_count,
        ));
        self
    }

    pub fn record_tag_basis(&mut self, rank: usize, conditioning: f32, explained_energy: f32) {
        self.tag_basis_rank = Some(rank);
        self.tag_basis_conditioning = Some(conditioning);
        self.tag_basis_explained_energy = Some(explained_energy);
    }

    pub fn merge(&mut self, other: Self) {
        self.channels_executed.extend(other.channels_executed);
        self.candidate_counts.extend(other.candidate_counts);
        if other.tag_basis_rank.is_some() {
            self.tag_basis_rank = other.tag_basis_rank;
            self.tag_basis_conditioning = other.tag_basis_conditioning;
            self.tag_basis_explained_energy = other.tag_basis_explained_energy;
        }
        self.activation_edge_visits = self
            .activation_edge_visits
            .saturating_add(other.activation_edge_visits);
        self.activation_hops = self.activation_hops.max(other.activation_hops);
        self.activation_truncated |= other.activation_truncated;
        self.diffusion_iterations = self
            .diffusion_iterations
            .saturating_add(other.diffusion_iterations);
        self.diffusion_convergence_delta = other
            .diffusion_convergence_delta
            .or(self.diffusion_convergence_delta);
        self.diffusion_truncated |= other.diffusion_truncated;
        self.independent_support_count = self
            .independent_support_count
            .saturating_add(other.independent_support_count);
        self.correlation_suppressed_evidence = self
            .correlation_suppressed_evidence
            .saturating_add(other.correlation_suppressed_evidence);
        self.relation_expansions = self
            .relation_expansions
            .saturating_add(other.relation_expansions);
        self.rerank_requested |= other.rerank_requested;
        self.rerank_applied |= other.rerank_applied;
        self.capability_degraded |= other.capability_degraded;
        self.authority_generation = other.authority_generation;
    }

    #[must_use]
    pub fn channel_executed(&self, channel: &str) -> bool {
        self.channels_executed.iter().any(|item| item == channel)
    }
}
