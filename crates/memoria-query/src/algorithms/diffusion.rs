use std::collections::{BTreeMap, BTreeSet};

use crate::TagSeed;
use crate::algorithms::activation::{PropagatedTag, PropagationBudget, PropagationError};
use crate::association::AssociationGraphView;

#[derive(Clone, Debug, PartialEq)]
pub struct DiffusionTrace {
    pub active_tags: Vec<PropagatedTag>,
    pub iterations: usize,
    pub convergence_delta: f32,
    pub converged: bool,
    pub edge_visits: usize,
    pub truncated: bool,
}

/// Run personalized graph diffusion over the bounded query-local graph.
///
/// This is intentionally distinct from best-path Activation: every iteration
/// distributes mass over normalized outgoing positive edge signals and
/// restarts from the original seed distribution.
pub fn diffusion_propagate(
    graph: &AssociationGraphView,
    seeds: &[TagSeed],
    budget: PropagationBudget,
) -> Result<DiffusionTrace, PropagationError> {
    if budget.max_active_tags == 0 || budget.max_edge_visits == 0 {
        return Err(PropagationError::InvalidBudget);
    }

    let mut node_ids = BTreeSet::new();
    let mut frontier = BTreeSet::new();
    let mut truncated = false;
    for seed in seeds {
        if node_ids.insert(seed.tag_id) {
            frontier.insert(seed.tag_id);
        }
    }
    for _ in 0..budget.max_hops {
        if frontier.is_empty() {
            break;
        }
        let mut next_frontier = BTreeSet::new();
        for source_id in frontier {
            for edge in graph.neighbors(source_id) {
                let target_id = if edge.left == source_id {
                    edge.right
                } else {
                    edge.left
                };
                if node_ids.len() >= budget.max_active_tags && !node_ids.contains(&target_id) {
                    truncated = true;
                    continue;
                }
                if node_ids.insert(target_id) {
                    next_frontier.insert(target_id);
                }
            }
        }
        frontier = next_frontier;
    }
    let node_ids = node_ids.into_iter().collect::<Vec<_>>();
    let mut indexes = BTreeMap::new();
    for (index, tag_id) in node_ids.iter().copied().enumerate() {
        indexes.insert(tag_id, index);
    }
    let mut seed_mass = vec![0.0_f32; node_ids.len()];
    let mut seed_origins = BTreeMap::<memoria_derived::TagId, Vec<crate::TagSeedProvenance>>::new();
    let mut seed_provenance =
        BTreeMap::<memoria_derived::TagId, Vec<crate::TagSeedProvenance>>::new();
    for seed in seeds {
        let provenances = seed_provenance.entry(seed.tag_id).or_default();
        if !provenances.contains(&seed.provenance) {
            provenances.push(seed.provenance);
        }
        if let Some(index) = indexes.get(&seed.tag_id).copied() {
            seed_mass[index] = seed_mass[index].max(seed.weight());
            let origins = seed_origins.entry(seed.tag_id).or_default();
            if !origins.contains(&seed.provenance) {
                origins.push(seed.provenance);
            }
        }
    }
    let total_seed_mass = seed_mass.iter().sum::<f32>();
    if total_seed_mass > 0.0 {
        for mass in &mut seed_mass {
            *mass /= total_seed_mass;
        }
    }

    let mut scores = seed_mass.clone();
    let mut support_sets = seed_mass
        .iter()
        .enumerate()
        .filter(|(_, score)| **score > 0.0)
        .map(|(index, _)| BTreeSet::from([node_ids[index]]))
        .collect::<Vec<_>>();
    if support_sets.len() != node_ids.len() {
        support_sets = vec![BTreeSet::new(); node_ids.len()];
        for seed in seeds {
            if let Some(index) = indexes.get(&seed.tag_id).copied() {
                support_sets[index].insert(seed.tag_id);
            }
        }
    }
    let mut edge_visits = 0;
    let mut iterations = 0;
    let mut convergence_delta = f32::INFINITY;
    let mut converged = false;
    while iterations < 8 {
        let mut next = seed_mass
            .iter()
            .map(|score| 0.35 * score)
            .collect::<Vec<_>>();
        let mut next_support = vec![BTreeSet::new(); node_ids.len()];
        for (index, score) in seed_mass.iter().copied().enumerate() {
            if score > 0.0 {
                next_support[index].extend(support_sets[index].iter().copied());
            }
        }
        for (source_index, source_id) in node_ids.iter().copied().enumerate() {
            let neighbors = graph.neighbors(source_id);
            let positive_total = neighbors
                .iter()
                .map(|edge| edge.signal())
                .filter(|signal| *signal > 0.0)
                .sum::<f32>();
            if positive_total <= 0.0 {
                continue;
            }
            for edge in neighbors {
                if edge_visits >= budget.max_edge_visits {
                    break;
                }
                edge_visits += 1;
                let signal = edge.signal();
                let target_id = if edge.left == source_id {
                    edge.right
                } else {
                    edge.left
                };
                let Some(target_index) = indexes.get(&target_id).copied() else {
                    continue;
                };
                if signal > 0.0 {
                    next[target_index] += 0.65 * scores[source_index] * signal / positive_total;
                    if scores[source_index] > 0.0 {
                        next_support[target_index]
                            .extend(support_sets[source_index].iter().copied());
                    }
                }
            }
            if edge_visits >= budget.max_edge_visits {
                break;
            }
        }
        convergence_delta = next
            .iter()
            .zip(&scores)
            .map(|(next, previous)| (next - previous).abs())
            .sum();
        scores = next;
        support_sets = next_support;
        iterations += 1;
        if convergence_delta <= 1.0e-5 {
            converged = true;
            break;
        }
        if edge_visits >= budget.max_edge_visits {
            break;
        }
    }

    let mut active_tags = node_ids
        .into_iter()
        .zip(scores)
        .filter(|(_, score)| *score > 0.0 && score.is_finite())
        .map(|(tag_id, score)| {
            let index = indexes[&tag_id];
            let support_seed_ids = support_sets[index].iter().copied().collect::<Vec<_>>();
            let mut origins = support_seed_ids
                .iter()
                .flat_map(|seed_id| seed_provenance.get(seed_id).into_iter().flatten().copied())
                .collect::<Vec<_>>();
            origins.sort_unstable();
            origins.dedup();
            if origins.is_empty() {
                origins = seed_origins.remove(&tag_id).unwrap_or_default();
            }
            PropagatedTag {
                tag_id,
                score,
                hops: 0,
                seed_origins: origins.clone(),
                strongest_support: score.clamp(0.0, 1.0),
                independent_seed_count: origins.len(),
                static_contribution: score,
                adaptive_contribution: 0.0,
                support_seed_ids,
                seed_ids: BTreeSet::from([tag_id]),
            }
        })
        .collect::<Vec<_>>();
    active_tags.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.tag_id.cmp(&right.tag_id))
    });
    Ok(DiffusionTrace {
        active_tags,
        iterations,
        convergence_delta,
        converged,
        edge_visits,
        truncated,
    })
}
