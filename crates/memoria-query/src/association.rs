use std::collections::BTreeMap;

use memoria_derived::TagId;
use thiserror::Error;

use crate::CompositeTagView;

/// Upper bound for the query-local association view.
pub const MAX_ASSOCIATION_EDGES: usize = 100_000;

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum AssociationError {
    #[error("association edge weight must be finite and non-negative")]
    InvalidWeight,

    #[error("association edge endpoints must be different")]
    SelfAssociation,

    #[error("association graph exceeds the edge limit {limit}")]
    EdgeLimit { limit: usize },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssociationEdge {
    pub left: TagId,
    pub right: TagId,
    pub weight: f32,
    pub static_weight: f32,
    pub adaptive_weight: f32,
}

impl AssociationEdge {
    #[must_use]
    pub fn total_weight(self) -> f32 {
        self.static_weight + self.adaptive_weight
    }

    #[must_use]
    pub fn signal(self) -> f32 {
        let weight = self.total_weight();
        if !weight.is_finite() || weight <= 0.0 {
            0.0
        } else {
            (weight / (1.0 + weight)).clamp(0.0, 1.0)
        }
    }
}

/// An immutable-at-query-time, Space-scoped association view.
///
/// The graph is built from an already scoped `CompositeTagView` or through
/// the bounded builder below. It contains no Authority mutation path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssociationGraph {
    edges: BTreeMap<(TagId, TagId), AssociationEdge>,
}

/// Alias used by callers that want to emphasize read-only query consumption.
pub type AssociationView = AssociationGraph;

impl AssociationGraph {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_composite(view: &CompositeTagView) -> Result<Self, AssociationError> {
        let mut graph = Self::new();
        for edge in view.associations() {
            graph.add_edge_with_components(
                edge.left,
                edge.right,
                edge.explicit_evidence() as f32,
                edge.generated_evidence() as f32 * 0.5,
            )?;
        }
        Ok(graph)
    }

    pub fn add_edge(
        &mut self,
        left: TagId,
        right: TagId,
        weight: f32,
    ) -> Result<(), AssociationError> {
        self.add_edge_with_components(left, right, weight, 0.0)
    }

    pub fn insert_edge(
        &mut self,
        left: TagId,
        right: TagId,
        weight: f32,
    ) -> Result<(), AssociationError> {
        self.add_edge(left, right, weight)
    }

    pub fn add_edge_with_components(
        &mut self,
        left: TagId,
        right: TagId,
        static_weight: f32,
        adaptive_weight: f32,
    ) -> Result<(), AssociationError> {
        if left == right {
            return Err(AssociationError::SelfAssociation);
        }
        if [static_weight, adaptive_weight]
            .into_iter()
            .any(|weight| !weight.is_finite() || weight < 0.0)
        {
            return Err(AssociationError::InvalidWeight);
        }
        let (left, right) = ordered_pair(left, right);
        let key = (left, right);
        let is_new = !self.edges.contains_key(&key);
        if is_new && self.edges.len() >= MAX_ASSOCIATION_EDGES {
            return Err(AssociationError::EdgeLimit {
                limit: MAX_ASSOCIATION_EDGES,
            });
        }
        let entry = self.edges.entry(key).or_insert(AssociationEdge {
            left,
            right,
            weight: 0.0,
            static_weight: 0.0,
            adaptive_weight: 0.0,
        });
        entry.static_weight = entry.static_weight.max(static_weight);
        entry.adaptive_weight = entry.adaptive_weight.max(adaptive_weight);
        entry.weight = entry.total_weight();
        Ok(())
    }

    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn edges(&self) -> impl Iterator<Item = &AssociationEdge> {
        self.edges.values()
    }

    #[must_use]
    pub fn neighbors(&self, tag_id: TagId) -> Vec<AssociationEdge> {
        self.edges
            .values()
            .filter(|edge| edge.left == tag_id || edge.right == tag_id)
            .copied()
            .collect()
    }
}

fn ordered_pair(left: TagId, right: TagId) -> (TagId, TagId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}
