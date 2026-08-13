use memoria_mdx::{IrNode, MemoryIr, SemanticKind, SemanticNodeId};

use crate::DerivedError;
use crate::projection::ProjectionTarget;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralArtifact {
    target: ProjectionTarget,
    entries: Vec<StructuralEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralEntry {
    pub target: ProjectionTarget,
    pub kind: SemanticKind,
    pub node_id: Option<SemanticNodeId>,
    pub parent_node_id: Option<SemanticNodeId>,
}

pub struct StructuralBuilder;

impl StructuralBuilder {
    pub fn build(ir: &MemoryIr) -> Result<StructuralArtifact, DerivedError> {
        Self::build_for(ir, ProjectionTarget::default())
    }

    pub fn build_for(
        ir: &MemoryIr,
        target: ProjectionTarget,
    ) -> Result<StructuralArtifact, DerivedError> {
        let entries = ir
            .nodes()
            .map(|node| StructuralEntry {
                target,
                kind: node.kind(),
                node_id: node.id().cloned(),
                parent_node_id: nearest_parent_id(ir, node),
            })
            .collect();
        Ok(StructuralArtifact { target, entries })
    }
}

impl StructuralArtifact {
    #[must_use]
    pub fn target(&self) -> ProjectionTarget {
        self.target
    }

    #[must_use]
    pub fn entries(&self) -> &[StructuralEntry] {
        &self.entries
    }
}

fn nearest_parent_id(ir: &MemoryIr, node: &IrNode) -> Option<SemanticNodeId> {
    let span = node.source_mapping().span();
    ir.nodes()
        .filter(|candidate| {
            candidate.id().is_some()
                && candidate.source_mapping().span().start <= span.start
                && span.end <= candidate.source_mapping().span().end
                && candidate.source_mapping().span() != span
        })
        .min_by_key(|candidate| {
            let candidate_span = candidate.source_mapping().span();
            candidate_span.end - candidate_span.start
        })
        .and_then(|candidate| candidate.id().cloned())
}
