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

    #[must_use]
    pub fn channel_executed(&self, channel: &str) -> bool {
        self.channels_executed.iter().any(|item| item == channel)
    }
}
