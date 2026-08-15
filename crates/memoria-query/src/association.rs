use std::collections::{BTreeSet, HashMap};

use memoria_derived::TagId;
use petgraph::Directed;
use petgraph::csr::Csr;
use petgraph::visit::{EdgeRef, IntoEdgeReferences};
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

    #[error("association graph contains duplicate endpoint {tag_id}")]
    DuplicateEndpoint { tag_id: TagId },

    #[error("association graph contains unknown endpoint {tag_id}")]
    UnknownEndpoint { tag_id: TagId },
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

/// Zero-sized node payload used by the CSR. The authoritative TagId mapping
/// stays in `index_to_tag`, so petgraph owns the adjacency storage while
/// Memoria retains stable Tag identity semantics.
#[derive(Clone, Copy, Debug, Default)]
struct TagNode;

/// An immutable-at-query-time, Space-scoped association view.
///
/// The graph is built from an already scoped `CompositeTagView` or through
/// the bounded builder below. It contains no Authority mutation path. The
/// directed CSR stores both directions of each undirected association so
/// propagation can use petgraph's native outgoing-neighbor and edge views.
#[derive(Clone, Debug)]
pub struct AssociationGraphView {
    graph: Csr<TagNode, AssociationEdge, Directed, u32>,
    tag_to_index: HashMap<TagId, u32>,
    index_to_tag: Vec<TagId>,
    edge_count: usize,
}

/// Existing callers use `AssociationGraph`; keep the name as a type alias
/// while making the concrete query view explicit for new integrations.
pub type AssociationGraph = AssociationGraphView;

/// Alias used by callers that want to emphasize read-only query consumption.
pub type AssociationView = AssociationGraphView;

impl Default for AssociationGraphView {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialEq for AssociationGraphView {
    fn eq(&self, other: &Self) -> bool {
        self.index_to_tag == other.index_to_tag
            && self.edge_count == other.edge_count
            && self.edges().eq(other.edges())
    }
}

impl AssociationGraphView {
    #[must_use]
    pub fn new() -> Self {
        Self::from_nodes_and_edges(&[], &[]).expect("empty association graph is valid")
    }

    /// Build a deterministic CSR from a complete node list and edge list.
    ///
    /// Nodes are sorted by stable TagId before assigning compact CSR indexes.
    /// Duplicate nodes and edge endpoints outside that complete list are
    /// rejected before any graph storage is constructed.
    pub fn from_nodes_and_edges(
        nodes: &[TagId],
        edges: &[AssociationEdge],
    ) -> Result<Self, AssociationError> {
        let mut index_to_tag = nodes.to_vec();
        index_to_tag.sort_unstable();
        for pair in index_to_tag.windows(2) {
            if pair[0] == pair[1] {
                return Err(AssociationError::DuplicateEndpoint { tag_id: pair[0] });
            }
        }

        let tag_to_index = index_to_tag
            .iter()
            .copied()
            .enumerate()
            .map(|(index, tag_id)| {
                let index = u32::try_from(index).map_err(|_| AssociationError::EdgeLimit {
                    limit: MAX_ASSOCIATION_EDGES,
                })?;
                Ok((tag_id, index))
            })
            .collect::<Result<HashMap<_, _>, AssociationError>>()?;

        let mut unique_edges = Vec::with_capacity(edges.len());
        for edge in edges {
            validate_components(edge.static_weight, edge.adaptive_weight)?;
            if edge.left == edge.right {
                return Err(AssociationError::SelfAssociation);
            }
            if !tag_to_index.contains_key(&edge.left) {
                return Err(AssociationError::UnknownEndpoint { tag_id: edge.left });
            }
            if !tag_to_index.contains_key(&edge.right) {
                return Err(AssociationError::UnknownEndpoint { tag_id: edge.right });
            }
            let (left, right) = ordered_pair(edge.left, edge.right);
            unique_edges.push(AssociationEdge {
                left,
                right,
                weight: edge.total_weight(),
                static_weight: edge.static_weight,
                adaptive_weight: edge.adaptive_weight,
            });
        }
        unique_edges.sort_by(|left, right| {
            left.left
                .cmp(&right.left)
                .then_with(|| left.right.cmp(&right.right))
        });
        let mut merged_edges = Vec::<AssociationEdge>::with_capacity(unique_edges.len());
        for edge in unique_edges {
            if let Some(existing) = merged_edges.last_mut()
                && existing.left == edge.left
                && existing.right == edge.right
            {
                existing.static_weight = existing.static_weight.max(edge.static_weight);
                existing.adaptive_weight = existing.adaptive_weight.max(edge.adaptive_weight);
                existing.weight = existing.total_weight();
                continue;
            }
            if merged_edges.len() >= MAX_ASSOCIATION_EDGES {
                return Err(AssociationError::EdgeLimit {
                    limit: MAX_ASSOCIATION_EDGES,
                });
            }
            merged_edges.push(edge);
        }

        let mut csr_edges = Vec::with_capacity(merged_edges.len().saturating_mul(2));
        for edge in &merged_edges {
            let left_index = tag_to_index[&edge.left];
            let right_index = tag_to_index[&edge.right];
            csr_edges.push((left_index, right_index, *edge));
            csr_edges.push((right_index, left_index, *edge));
        }
        csr_edges.sort_by_key(|(source, target, _)| (*source, *target));
        let mut graph = Csr::from_sorted_edges(&csr_edges)
            .expect("CSR edge tuples are sorted and unique by construction");
        while graph.node_count() < index_to_tag.len() {
            graph.add_node(TagNode);
        }

        Ok(Self {
            graph,
            tag_to_index,
            index_to_tag,
            edge_count: merged_edges.len(),
        })
    }

    /// Short alias for callers that already have a complete graph edge list.
    pub fn from_edges(
        nodes: &[TagId],
        edges: &[AssociationEdge],
    ) -> Result<Self, AssociationError> {
        Self::from_nodes_and_edges(nodes, edges)
    }

    pub fn from_composite(view: &CompositeTagView) -> Result<Self, AssociationError> {
        let mut nodes = BTreeSet::new();
        let edges = view
            .associations()
            .map(|edge| {
                nodes.insert(edge.left);
                nodes.insert(edge.right);
                AssociationEdge {
                    left: edge.left,
                    right: edge.right,
                    weight: edge.weight(),
                    static_weight: edge.explicit_evidence() as f32,
                    adaptive_weight: edge.generated_evidence() as f32 * 0.5,
                }
            })
            .collect::<Vec<_>>();
        Self::from_nodes_and_edges(&nodes.into_iter().collect::<Vec<_>>(), &edges)
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
        validate_components(static_weight, adaptive_weight)?;

        let (left, right) = ordered_pair(left, right);
        let existing = self.existing_edge(left, right);
        if let Some(existing) = existing {
            let next_static = existing.static_weight.max(static_weight);
            let next_adaptive = existing.adaptive_weight.max(adaptive_weight);
            if next_static == existing.static_weight && next_adaptive == existing.adaptive_weight {
                return Ok(());
            }
            let nodes = self.index_to_tag.clone();
            let mut edges = self.edges().copied().collect::<Vec<_>>();
            let stored = edges
                .iter_mut()
                .find(|edge| edge.left == left && edge.right == right)
                .expect("existing CSR edge must be present in the logical edge view");
            stored.static_weight = next_static;
            stored.adaptive_weight = next_adaptive;
            stored.weight = stored.total_weight();
            *self = Self::from_nodes_and_edges(&nodes, &edges)?;
            return Ok(());
        }

        if self.edge_count >= MAX_ASSOCIATION_EDGES {
            return Err(AssociationError::EdgeLimit {
                limit: MAX_ASSOCIATION_EDGES,
            });
        }
        let left_index = self.ensure_node(left);
        let right_index = self.ensure_node(right);
        let edge = AssociationEdge {
            left,
            right,
            weight: static_weight + adaptive_weight,
            static_weight,
            adaptive_weight,
        };
        debug_assert!(self.graph.add_edge(left_index, right_index, edge));
        debug_assert!(self.graph.add_edge(right_index, left_index, edge));
        self.edge_count += 1;
        Ok(())
    }

    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    pub fn edges(&self) -> impl Iterator<Item = &AssociationEdge> {
        self.graph.edge_references().filter_map(|edge| {
            let source = self.index_to_tag[edge.source() as usize];
            (source == edge.weight().left).then_some(edge.weight())
        })
    }

    #[must_use]
    pub fn neighbors(&self, tag_id: TagId) -> Vec<AssociationEdge> {
        let Some(index) = self.tag_to_index.get(&tag_id).copied() else {
            return Vec::new();
        };
        self.graph.edges(index).map(|edge| *edge.weight()).collect()
    }

    fn existing_edge(&self, left: TagId, right: TagId) -> Option<AssociationEdge> {
        let left_index = self.tag_to_index.get(&left).copied()?;
        let right_index = self.tag_to_index.get(&right).copied()?;
        self.graph
            .edges(left_index)
            .find(|edge| edge.target() == right_index)
            .map(|edge| *edge.weight())
    }

    fn ensure_node(&mut self, tag_id: TagId) -> u32 {
        if let Some(index) = self.tag_to_index.get(&tag_id).copied() {
            return index;
        }
        let index = u32::try_from(self.index_to_tag.len())
            .expect("query-local association node index must fit in u32");
        self.graph.add_node(TagNode);
        self.tag_to_index.insert(tag_id, index);
        self.index_to_tag.push(tag_id);
        index
    }
}

fn validate_components(static_weight: f32, adaptive_weight: f32) -> Result<(), AssociationError> {
    if [static_weight, adaptive_weight]
        .into_iter()
        .any(|weight| !weight.is_finite() || weight < 0.0)
    {
        return Err(AssociationError::InvalidWeight);
    }
    Ok(())
}

fn ordered_pair(left: TagId, right: TagId) -> (TagId, TagId) {
    if left <= right {
        (left, right)
    } else {
        (right, left)
    }
}
