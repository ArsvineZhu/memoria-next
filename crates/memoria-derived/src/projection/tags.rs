use memoria_mdx::{MemoryIr, SemanticKind};

use crate::DerivedError;
use crate::projection::ProjectionTarget;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TagProvenance {
    Explicit,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExplicitTagArtifact {
    target: ProjectionTarget,
    memberships: Vec<TagMembership>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagMembership {
    pub target: ProjectionTarget,
    pub value: String,
    pub node_id: Option<String>,
    pub provenance: TagProvenance,
}

pub struct ExplicitTagBuilder;

impl ExplicitTagBuilder {
    pub fn build(ir: &MemoryIr) -> Result<ExplicitTagArtifact, DerivedError> {
        Self::build_for(ir, ProjectionTarget::default())
    }

    pub fn build_for(
        ir: &MemoryIr,
        target: ProjectionTarget,
    ) -> Result<ExplicitTagArtifact, DerivedError> {
        let memberships = ir
            .nodes()
            .filter(|node| node.kind() == SemanticKind::Tag)
            .map(|node| {
                let value = attribute(node, "value")
                    .ok_or_else(|| DerivedError::InvalidProjectionValue {
                        value: "Tag.value is missing".to_owned(),
                    })?
                    .to_owned();
                Ok(TagMembership {
                    target,
                    value,
                    node_id: nearest_scope_id(ir, node),
                    provenance: TagProvenance::Explicit,
                })
            })
            .collect::<Result<Vec<_>, DerivedError>>()?;
        Ok(ExplicitTagArtifact {
            target,
            memberships,
        })
    }
}

impl ExplicitTagArtifact {
    #[must_use]
    pub fn target(&self) -> ProjectionTarget {
        self.target
    }

    #[must_use]
    pub fn memberships(&self) -> &[TagMembership] {
        &self.memberships
    }
}

fn nearest_scope_id(ir: &MemoryIr, node: &memoria_mdx::IrNode) -> Option<String> {
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
        .and_then(|candidate| candidate.id().map(ToString::to_string))
}

fn attribute<'a>(node: &'a memoria_mdx::IrNode, name: &str) -> Option<&'a str> {
    node.attributes()
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}
