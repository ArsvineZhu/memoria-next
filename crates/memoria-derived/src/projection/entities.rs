use memoria_mdx::{MemoryIr, SemanticKind, SemanticNodeId};

use crate::DerivedError;
use crate::projection::ProjectionTarget;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityRef(String);

impl EntityRef {
    pub fn new(value: impl Into<String>) -> Result<Self, DerivedError> {
        let value = value.into();
        if value.is_empty() || !value.contains(':') || value.chars().any(char::is_whitespace) {
            return Err(DerivedError::InvalidProjectionValue { value });
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityObservationArtifact {
    target: ProjectionTarget,
    observations: Vec<EntityObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityObservation {
    pub target: ProjectionTarget,
    pub entity_ref: EntityRef,
    pub surface: String,
    pub node_id: Option<SemanticNodeId>,
}

pub struct EntityObservationBuilder;

impl EntityObservationBuilder {
    pub fn build(ir: &MemoryIr) -> Result<EntityObservationArtifact, DerivedError> {
        Self::build_for(ir, ProjectionTarget::default())
    }

    pub fn build_for(
        ir: &MemoryIr,
        target: ProjectionTarget,
    ) -> Result<EntityObservationArtifact, DerivedError> {
        let observations = ir
            .nodes()
            .filter(|node| node.kind() == SemanticKind::Entity)
            .map(|node| {
                let entity_ref = attribute(node, "ref")
                    .ok_or_else(|| DerivedError::InvalidProjectionValue {
                        value: "Entity.ref is missing".to_owned(),
                    })
                    .and_then(EntityRef::new)?;
                Ok(EntityObservation {
                    target,
                    entity_ref,
                    surface: node.text().to_owned(),
                    node_id: node.id().cloned(),
                })
            })
            .collect::<Result<Vec<_>, DerivedError>>()?;
        Ok(EntityObservationArtifact {
            target,
            observations,
        })
    }
}

impl EntityObservationArtifact {
    #[must_use]
    pub fn target(&self) -> ProjectionTarget {
        self.target
    }

    #[must_use]
    pub fn observations(&self) -> &[EntityObservation] {
        &self.observations
    }
}

fn attribute<'a>(node: &'a memoria_mdx::IrNode, name: &str) -> Option<&'a str> {
    node.attributes()
        .iter()
        .find(|(candidate, _)| candidate == name)
        .map(|(_, value)| value.as_str())
}
