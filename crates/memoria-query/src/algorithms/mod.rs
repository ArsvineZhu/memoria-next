mod activation;
mod diffusion;
mod structure;
mod support;
mod tag_basis;

pub use activation::{
    PropagatedTag, PropagationBudget, PropagationError, PropagationTrace, activation_propagate,
};
pub use diffusion::diffusion_propagate;
pub use structure::{StructureEvidence, collect_structure};
pub use support::{SupportEvidence, collect_support};
pub use tag_basis::{
    MAX_TAG_BASIS_DIMENSIONS, MAX_TAG_BASIS_VECTORS, TagBasisError, TagBasisResult, l2_norm,
    project_tag_basis,
};
