use memoria_derived::TagId;

use crate::algorithms::activation::PropagationTrace;
use crate::association::AssociationGraph;
use crate::semantic::SemanticResolution;

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

/// Bounded ranking bonus for structural diversity and short paths.
#[must_use]
pub fn structure_bonus(path_diversity: f32, min_hops: usize, resolution_uniqueness: bool) -> f32 {
    let path_diversity = path_diversity.clamp(0.0, 1.0);
    let hop_quality = 1.0 / (1.0 + min_hops as f32);
    let resolution_component = f32::from(resolution_uniqueness);
    (0.03 * path_diversity + 0.03 * hop_quality + 0.02 * resolution_component).clamp(0.0, 0.08)
}

/// Parent/child/section resolutions for one target are correlated evidence,
/// not independent support paths.
#[must_use]
pub fn correlated_resolutions(resolutions: &[SemanticResolution]) -> bool {
    resolutions.len() > 1
        && resolutions
            .iter()
            .any(|resolution| *resolution != resolutions[0])
}
