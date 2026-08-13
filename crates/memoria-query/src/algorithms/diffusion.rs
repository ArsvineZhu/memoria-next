use crate::TagSeed;
use crate::algorithms::activation::{
    PropagationBudget, PropagationError, PropagationTrace, activation_propagate,
};
use crate::association::AssociationGraph;

/// Run the bounded diffusion baseline over the same query-local association
/// view. Keeping the budget and trace identical makes ablation accounting
/// explicit while leaving adaptive weighting isolated from Authority state.
pub fn diffusion_propagate(
    graph: &AssociationGraph,
    seeds: &[TagSeed],
    budget: PropagationBudget,
) -> Result<PropagationTrace, PropagationError> {
    activation_propagate(graph, seeds, budget)
}
