mod activation;
mod diffusion;
mod structure;
mod support;
mod tag_basis;

pub use activation::{
    PropagatedTag, PropagationBudget, PropagationError, PropagationTrace, activation_propagate,
};
pub use diffusion::{DiffusionTrace, diffusion_propagate};
pub use structure::{
    StructureEvidence, collect_structure, correlated_resolutions, structure_bonus,
};
pub use support::{SupportEvidence, collect_support, support_bonus};
pub use tag_basis::{
    BALANCED_TAG_BASIS_VECTORS, FAST_TAG_BASIS_VECTORS, MAX_TAG_BASIS_DIMENSIONS,
    MAX_TAG_BASIS_VECTORS, THOROUGH_TAG_BASIS_VECTORS, TagBasisError, TagBasisResult, l2_norm,
    project_tag_basis, project_tag_basis_with_limit,
};
