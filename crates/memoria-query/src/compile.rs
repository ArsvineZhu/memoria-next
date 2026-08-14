use memoria_derived::{DerivedManifest, ManifestId};
use memoria_types::AuthorityGeneration;

use crate::model::MemoryQuery;
use crate::planner::{CapabilityExecution, CapabilityPlanner};
use crate::snapshot::{AdaptiveSnapshotIdentity, QuerySnapshot};
use crate::validate::QueryError;

#[derive(Clone, Debug, PartialEq)]
pub struct CompiledQuery {
    pub query: MemoryQuery,
    pub snapshot: QuerySnapshot,
    pub execution: CapabilityExecution,
}

#[derive(Clone, Debug)]
pub struct QueryCompiler {
    current_authority: AuthorityGeneration,
    manifest: Option<DerivedManifest>,
    adaptive: AdaptiveSnapshotIdentity,
}

impl QueryCompiler {
    #[must_use]
    pub fn new(current_authority: AuthorityGeneration, manifest: Option<DerivedManifest>) -> Self {
        Self {
            current_authority,
            manifest,
            adaptive: AdaptiveSnapshotIdentity::Disabled,
        }
    }

    #[must_use]
    pub fn with_adaptive_snapshot(mut self, adaptive: AdaptiveSnapshotIdentity) -> Self {
        self.adaptive = adaptive;
        self
    }

    pub fn compile(&self, query: MemoryQuery) -> Result<CompiledQuery, QueryError> {
        query.validate()?;
        let target = CapabilityPlanner::target(&query, self.current_authority)?;
        let manifest = self
            .manifest
            .as_ref()
            .ok_or(QueryError::DerivedSnapshotUnavailable)?;
        let execution = CapabilityPlanner::capabilities(&query, manifest, target)?;
        Ok(CompiledQuery {
            query,
            snapshot: QuerySnapshot {
                authority_generation: target.authority_generation,
                derived_manifest: manifest.id(),
                adaptive: self.adaptive.clone(),
            },
            execution,
        })
    }

    #[must_use]
    pub fn current_authority(&self) -> AuthorityGeneration {
        self.current_authority
    }

    #[must_use]
    pub fn manifest_id(&self) -> Option<ManifestId> {
        self.manifest.as_ref().map(DerivedManifest::id)
    }
}
