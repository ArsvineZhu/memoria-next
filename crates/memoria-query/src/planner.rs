use memoria_derived::DerivedManifest;
use memoria_types::AuthorityGeneration;

use crate::model::{AuthorityConsistency, MemoryQuery, QueryQualityLevel};
use crate::validate::QueryError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityExecution {
    pub degraded: bool,
    pub used_capabilities: Vec<String>,
    pub degraded_capabilities: Vec<String>,
}

impl CapabilityExecution {
    #[must_use]
    pub fn used(&self, capability: &str) -> bool {
        self.used_capabilities.iter().any(|item| item == capability)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityTarget {
    pub authority_generation: AuthorityGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetrievalProfile {
    pub lexical_candidates: usize,
    pub semantic_direct_candidates: usize,
    pub semantic_residual_candidates: usize,
    pub tag_readout_candidates: usize,
    pub tag_basis_vectors: usize,
    pub activation_budget: crate::PropagationBudget,
    pub diffusion_max_nodes: usize,
    pub relation_budget: crate::RelationExpansionBudget,
    pub run_diffusion: bool,
    pub rerank_candidates: usize,
}

impl RetrievalProfile {
    #[must_use]
    pub const fn for_quality(level: QueryQualityLevel) -> Self {
        match level {
            QueryQualityLevel::Fast => Self {
                lexical_candidates: 32,
                semantic_direct_candidates: 32,
                semantic_residual_candidates: 16,
                tag_readout_candidates: 24,
                tag_basis_vectors: crate::FAST_TAG_BASIS_VECTORS,
                activation_budget: crate::PropagationBudget {
                    max_active_tags: 48,
                    max_edge_visits: 512,
                    max_hops: 2,
                },
                diffusion_max_nodes: 0,
                relation_budget: crate::RelationExpansionBudget {
                    max_hops: 1,
                    max_added: 16,
                },
                run_diffusion: false,
                rerank_candidates: 16,
            },
            QueryQualityLevel::Balanced => Self {
                lexical_candidates: 64,
                semantic_direct_candidates: 64,
                semantic_residual_candidates: 32,
                tag_readout_candidates: 48,
                tag_basis_vectors: crate::BALANCED_TAG_BASIS_VECTORS,
                activation_budget: crate::PropagationBudget {
                    max_active_tags: 96,
                    max_edge_visits: 1024,
                    max_hops: 3,
                },
                diffusion_max_nodes: 0,
                relation_budget: crate::RelationExpansionBudget {
                    max_hops: 1,
                    max_added: 32,
                },
                run_diffusion: false,
                rerank_candidates: 32,
            },
            QueryQualityLevel::Thorough => Self {
                lexical_candidates: 128,
                semantic_direct_candidates: 128,
                semantic_residual_candidates: 64,
                tag_readout_candidates: 96,
                tag_basis_vectors: crate::THOROUGH_TAG_BASIS_VECTORS,
                activation_budget: crate::PropagationBudget {
                    max_active_tags: 192,
                    max_edge_visits: 4096,
                    max_hops: 4,
                },
                diffusion_max_nodes: 256,
                relation_budget: crate::RelationExpansionBudget {
                    max_hops: 2,
                    max_added: 64,
                },
                run_diffusion: true,
                rerank_candidates: 64,
            },
        }
    }
}

pub struct CapabilityPlanner;

impl CapabilityPlanner {
    #[must_use]
    pub const fn retrieval_profile(query: &MemoryQuery) -> RetrievalProfile {
        RetrievalProfile::for_quality(query.quality.level)
    }
    #[must_use]
    pub fn prefers_lexical(query: &MemoryQuery) -> bool {
        !query.cue.text.is_empty()
    }

    pub fn target(
        query: &MemoryQuery,
        current_authority: AuthorityGeneration,
    ) -> Result<CapabilityTarget, QueryError> {
        let authority_generation = match query.consistency.authority {
            AuthorityConsistency::Latest => current_authority,
            AuthorityConsistency::AtLeast(required) => {
                if current_authority < required {
                    return Err(QueryError::AuthorityNotReady {
                        required,
                        actual: current_authority,
                    });
                }
                current_authority
            }
            AuthorityConsistency::Pinned(pinned) => {
                if current_authority < pinned {
                    return Err(QueryError::AuthorityNotReady {
                        required: pinned,
                        actual: current_authority,
                    });
                }
                pinned
            }
        };
        Ok(CapabilityTarget {
            authority_generation,
        })
    }

    pub fn capabilities(
        query: &MemoryQuery,
        manifest: &DerivedManifest,
        target: CapabilityTarget,
    ) -> Result<CapabilityExecution, QueryError> {
        let mut used_capabilities = Vec::new();
        let mut degraded_capabilities = Vec::new();
        for capability in &query.required_capabilities {
            ensure_ready(manifest, capability, target.authority_generation)?;
            used_capabilities.push(capability.clone());
        }
        for capability in &query.preferred_capabilities {
            if is_ready(manifest, capability, target.authority_generation) {
                used_capabilities.push(capability.clone());
            } else {
                degraded_capabilities.push(capability.clone());
            }
        }
        Ok(CapabilityExecution {
            degraded: !degraded_capabilities.is_empty(),
            used_capabilities,
            degraded_capabilities,
        })
    }
}

fn ensure_ready(
    manifest: &DerivedManifest,
    capability: &str,
    required_generation: AuthorityGeneration,
) -> Result<(), QueryError> {
    if is_ready(manifest, capability, required_generation) {
        return Ok(());
    }
    let available = manifest.capability(capability).coverage();
    Err(QueryError::CapabilityNotReady {
        capability: capability.to_owned(),
        required: required_generation,
        available,
    })
}

fn is_ready(
    manifest: &DerivedManifest,
    capability: &str,
    required_generation: AuthorityGeneration,
) -> bool {
    let status = manifest.capability(capability);
    status.is_ready() && status.coverage() >= required_generation
}
