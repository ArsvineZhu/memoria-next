use memoria_derived::DerivedManifest;
use memoria_types::AuthorityGeneration;

use crate::model::{AuthorityConsistency, MemoryQuery};
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

pub struct CapabilityPlanner;

impl CapabilityPlanner {
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
