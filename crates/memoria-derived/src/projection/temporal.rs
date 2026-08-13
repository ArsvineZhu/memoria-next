use memoria_mdx::{MemoryIr, SemanticKind, SemanticNodeId, TemporalValue};

use crate::DerivedError;
use crate::projection::ProjectionTarget;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporalArtifact {
    target: ProjectionTarget,
    assertions: Vec<TemporalAssertion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporalAssertion {
    pub target: ProjectionTarget,
    pub kind: SemanticKind,
    pub node_id: Option<SemanticNodeId>,
    pub occurred_at: Option<TemporalValue>,
    pub observed_at: Option<TemporalValue>,
    pub valid_from: Option<TemporalValue>,
    pub valid_to: Option<TemporalValue>,
}

pub struct TemporalBuilder;

impl TemporalBuilder {
    pub fn build(ir: &MemoryIr) -> Result<TemporalArtifact, DerivedError> {
        Self::build_for(ir, ProjectionTarget::default())
    }

    pub fn build_for(
        ir: &MemoryIr,
        target: ProjectionTarget,
    ) -> Result<TemporalArtifact, DerivedError> {
        let assertions = ir
            .nodes()
            .filter(|node| {
                node.occurred_at().is_some()
                    || node.observed_at().is_some()
                    || node.valid_from().is_some()
                    || node.valid_to().is_some()
            })
            .map(|node| TemporalAssertion {
                target,
                kind: node.kind(),
                node_id: node.id().cloned(),
                occurred_at: node.occurred_at().cloned(),
                observed_at: node.observed_at().cloned(),
                valid_from: node.valid_from().cloned(),
                valid_to: node.valid_to().cloned(),
            })
            .collect();
        Ok(TemporalArtifact { target, assertions })
    }
}

impl TemporalArtifact {
    #[must_use]
    pub fn target(&self) -> ProjectionTarget {
        self.target
    }

    #[must_use]
    pub fn assertions(&self) -> &[TemporalAssertion] {
        &self.assertions
    }
}
