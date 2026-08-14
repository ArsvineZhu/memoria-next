use std::collections::{BTreeMap, BTreeSet, VecDeque};

use memoria_derived::TagId;
use thiserror::Error;

use crate::association::AssociationGraph;
use crate::{TagSeed, TagSeedProvenance};

const HOP_DECAY: f32 = 0.5;
const SCORE_EPSILON: f32 = 1.0e-6;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropagationBudget {
    pub max_active_tags: usize,
    pub max_edge_visits: usize,
    pub max_hops: usize,
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PropagationError {
    #[error("propagation budget values must be greater than zero")]
    InvalidBudget,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropagatedTag {
    pub tag_id: TagId,
    pub score: f32,
    pub hops: usize,
    pub seed_origins: Vec<TagSeedProvenance>,
    pub strongest_support: f32,
    pub independent_seed_count: usize,
    pub static_contribution: f32,
    pub adaptive_contribution: f32,
    /// Unique seed Tag IDs that independently support this propagated Tag.
    pub support_seed_ids: Vec<TagId>,
    pub(crate) seed_ids: BTreeSet<TagId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PropagationTrace {
    pub active_tags: Vec<PropagatedTag>,
    pub edge_visits: usize,
    pub max_hops_reached: usize,
    pub truncated: bool,
}

#[derive(Clone, Copy, Debug)]
struct FrontierItem {
    tag_id: TagId,
    score: f32,
    hops: usize,
}

pub fn activation_propagate(
    graph: &AssociationGraph,
    seeds: &[TagSeed],
    budget: PropagationBudget,
) -> Result<PropagationTrace, PropagationError> {
    if budget.max_active_tags == 0 || budget.max_edge_visits == 0 {
        return Err(PropagationError::InvalidBudget);
    }

    let mut active = BTreeMap::<TagId, PropagatedTag>::new();
    let mut frontier = VecDeque::new();
    let mut truncated = false;
    for seed in seeds {
        if active.contains_key(&seed.tag_id) {
            continue;
        }
        if active.len() >= budget.max_active_tags {
            truncated = true;
            break;
        }
        active.insert(
            seed.tag_id,
            PropagatedTag {
                tag_id: seed.tag_id,
                score: 1.0,
                hops: 0,
                seed_origins: vec![seed.provenance],
                strongest_support: 1.0,
                independent_seed_count: 1,
                static_contribution: 1.0,
                adaptive_contribution: 0.0,
                support_seed_ids: vec![seed.tag_id],
                seed_ids: BTreeSet::from([seed.tag_id]),
            },
        );
        frontier.push_back(FrontierItem {
            tag_id: seed.tag_id,
            score: 1.0,
            hops: 0,
        });
    }

    let mut edge_visits = 0;
    let mut max_hops_reached = 0;
    while let Some(item) = frontier.pop_front() {
        max_hops_reached = max_hops_reached.max(item.hops);
        if item.hops >= budget.max_hops {
            continue;
        }
        for edge in graph.neighbors(item.tag_id) {
            if edge_visits >= budget.max_edge_visits {
                truncated = true;
                break;
            }
            edge_visits += 1;
            let target = if edge.left == item.tag_id {
                edge.right
            } else {
                edge.left
            };
            let signal = edge.signal();
            let score = item.score * signal * HOP_DECAY;
            if score <= SCORE_EPSILON {
                continue;
            }
            let parent = active[&item.tag_id].clone();

            let mut should_enqueue = false;
            if let Some(existing) = active.get_mut(&target) {
                existing.strongest_support = existing.strongest_support.max(signal);
                existing.seed_ids.extend(parent.seed_ids.iter().copied());
                existing.independent_seed_count = existing.seed_ids.len();
                existing.support_seed_ids = existing.seed_ids.iter().copied().collect();
                merge_origins(&mut existing.seed_origins, &parent.seed_origins);
                existing.static_contribution = existing
                    .static_contribution
                    .max(item.score * edge.static_weight);
                existing.adaptive_contribution = existing
                    .adaptive_contribution
                    .max(item.score * edge.adaptive_weight);
                if score > existing.score + SCORE_EPSILON {
                    existing.score = score;
                    existing.hops = item.hops + 1;
                    should_enqueue = true;
                }
            } else {
                if active.len() >= budget.max_active_tags {
                    truncated = true;
                    continue;
                }
                active.insert(
                    target,
                    PropagatedTag {
                        tag_id: target,
                        score,
                        hops: item.hops + 1,
                        seed_origins: parent.seed_origins,
                        strongest_support: signal,
                        independent_seed_count: parent.independent_seed_count,
                        static_contribution: item.score * edge.static_weight,
                        adaptive_contribution: item.score * edge.adaptive_weight,
                        support_seed_ids: parent.seed_ids.iter().copied().collect(),
                        seed_ids: parent.seed_ids,
                    },
                );
                should_enqueue = true;
            }
            if should_enqueue {
                frontier.push_back(FrontierItem {
                    tag_id: target,
                    score,
                    hops: item.hops + 1,
                });
            }
        }
        if edge_visits >= budget.max_edge_visits {
            truncated = true;
            break;
        }
    }

    let mut active_tags = active.into_values().collect::<Vec<_>>();
    active_tags.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.tag_id.cmp(&right.tag_id))
    });
    Ok(PropagationTrace {
        active_tags,
        edge_visits,
        max_hops_reached,
        truncated,
    })
}

fn merge_origins(target: &mut Vec<TagSeedProvenance>, source: &[TagSeedProvenance]) {
    let mut origins = target.iter().copied().collect::<BTreeSet<_>>();
    origins.extend(source.iter().copied());
    *target = origins.into_iter().collect();
}
