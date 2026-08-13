use memoria_derived::TagId;

use crate::algorithms::activation::PropagationTrace;
use crate::association::AssociationGraph;

#[derive(Clone, Debug, PartialEq)]
pub struct StructureEvidence {
    pub tag_id: TagId,
    pub degree: usize,
    pub mean_weight: f32,
}

#[must_use]
pub fn collect_structure(
    graph: &AssociationGraph,
    trace: &PropagationTrace,
    max_tags: usize,
) -> Vec<StructureEvidence> {
    trace
        .active_tags
        .iter()
        .take(max_tags)
        .map(|tag| {
            let neighbors = graph.neighbors(tag.tag_id);
            let degree = neighbors.len();
            let mean_weight = if degree == 0 {
                0.0
            } else {
                neighbors
                    .iter()
                    .map(|edge| edge.total_weight())
                    .sum::<f32>()
                    / degree as f32
            };
            StructureEvidence {
                tag_id: tag.tag_id,
                degree,
                mean_weight,
            }
        })
        .collect()
}
