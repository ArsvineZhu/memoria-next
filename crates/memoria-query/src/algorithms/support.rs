use memoria_derived::TagId;

use crate::TagSeedProvenance;
use crate::algorithms::activation::{PropagatedTag, PropagationTrace};

#[derive(Clone, Debug, PartialEq)]
pub struct SupportEvidence {
    pub tag_id: TagId,
    pub score: f32,
    pub seed_origins: Vec<TagSeedProvenance>,
    pub strongest_support: f32,
    pub independent_seed_count: usize,
    pub static_contribution: f32,
    pub adaptive_contribution: f32,
}

#[must_use]
pub fn collect_support(trace: &PropagationTrace) -> Vec<SupportEvidence> {
    trace
        .active_tags
        .iter()
        .map(SupportEvidence::from)
        .collect()
}

impl From<&PropagatedTag> for SupportEvidence {
    fn from(tag: &PropagatedTag) -> Self {
        Self {
            tag_id: tag.tag_id,
            score: tag.score,
            seed_origins: tag.seed_origins.clone(),
            strongest_support: tag.strongest_support,
            independent_seed_count: tag.independent_seed_count,
            static_contribution: tag.static_contribution,
            adaptive_contribution: tag.adaptive_contribution,
        }
    }
}
