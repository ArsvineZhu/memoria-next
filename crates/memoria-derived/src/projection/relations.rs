use memoria_mdx::{MemoryIr, SemanticNodeId};

use crate::DerivedError;
use crate::projection::ProjectionTarget;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelationArtifact {
    target: ProjectionTarget,
    relations: Vec<RelationRecord>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelationRecord {
    pub target: ProjectionTarget,
    pub node_id: SemanticNodeId,
    pub from: SemanticNodeId,
    pub to: SemanticNodeId,
    pub kind: String,
}

pub struct RelationBuilder;

impl RelationBuilder {
    pub fn build(ir: &MemoryIr) -> Result<RelationArtifact, DerivedError> {
        Self::build_for(ir, ProjectionTarget::default())
    }

    pub fn build_for(
        ir: &MemoryIr,
        target: ProjectionTarget,
    ) -> Result<RelationArtifact, DerivedError> {
        let relations = ir
            .nodes()
            .filter(|node| node.kind() == memoria_mdx::SemanticKind::Relation)
            .map(|node| {
                let node_id =
                    node.id()
                        .cloned()
                        .ok_or_else(|| memoria_mdx::MdxError::InvalidNodeId {
                            value: String::new(),
                            span: node.source_mapping().span(),
                        })?;
                let from = local_id(node, "from")?;
                let to = local_id(node, "to")?;
                let kind = attribute(node, "kind").unwrap_or("association").to_owned();
                Ok(RelationRecord {
                    target,
                    node_id,
                    from,
                    to,
                    kind,
                })
            })
            .collect::<Result<Vec<_>, memoria_mdx::MdxError>>()?;
        Ok(RelationArtifact { target, relations })
    }
}

impl RelationArtifact {
    #[must_use]
    pub fn target(&self) -> ProjectionTarget {
        self.target
    }

    #[must_use]
    pub fn relations(&self) -> &[RelationRecord] {
        &self.relations
    }
}

fn local_id(
    node: &memoria_mdx::IrNode,
    name: &str,
) -> Result<SemanticNodeId, memoria_mdx::MdxError> {
    let value = attribute(node, name).unwrap_or_default();
    value.strip_prefix('#').unwrap_or(value).parse()
}

fn attribute<'a>(node: &'a memoria_mdx::IrNode, name: &str) -> Option<&'a str> {
    node.attributes()
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}
