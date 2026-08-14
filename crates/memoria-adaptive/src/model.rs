use std::collections::BTreeMap;

use memoria_types::{AdaptiveGeneration, MemoryId, RevisionId, SpaceId, Timestamp};
use serde::{Deserialize, Serialize};

const SATURATION_SCALE: f64 = 4.0;
const RECENCY_HALF_LIFE_SECONDS: f64 = 30.0 * 24.0 * 60.0 * 60.0;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveStateV1 {
    targets: BTreeMap<(SpaceId, MemoryId), TargetFamiliarity>,
    tag_affinity: BTreeMap<(SpaceId, MemoryId, String), AffinityStats>,
    query_class_affinity: BTreeMap<(SpaceId, MemoryId, String), AffinityStats>,
    generation: AdaptiveGeneration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AdaptiveReadSnapshot {
    pub generation: AdaptiveGeneration,
    pub model_version: String,
    state: AdaptiveStateV1,
}

impl AdaptiveReadSnapshot {
    pub(crate) fn new(generation: AdaptiveGeneration, state: AdaptiveStateV1) -> Self {
        Self {
            generation,
            model_version: AdaptiveStateV1::model_version().to_owned(),
            state,
        }
    }

    #[must_use]
    pub const fn state(&self) -> &AdaptiveStateV1 {
        &self.state
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TargetFamiliarity {
    pub success_count: u64,
    pub negative_count: u64,
    pub positive_weight: f64,
    pub last_success_at: Option<Timestamp>,
    pub last_feedback_generation: AdaptiveGeneration,
    revisions: BTreeMap<RevisionId, RevisionFamiliarity>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RevisionFamiliarity {
    pub success_count: u64,
    pub last_success_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AffinityStats {
    pub positive_count: u64,
    pub negative_count: u64,
    pub positive_weight: f64,
    pub negative_weight: f64,
}

impl AdaptiveStateV1 {
    #[must_use]
    pub const fn model_version() -> &'static str {
        "adaptive-v1"
    }

    #[must_use]
    pub const fn generation(&self) -> AdaptiveGeneration {
        self.generation
    }

    #[must_use]
    pub fn familiarity(
        &self,
        space_id: SpaceId,
        memory_id: MemoryId,
    ) -> Option<&TargetFamiliarity> {
        self.targets.get(&(space_id, memory_id))
    }

    #[must_use]
    pub fn target_strength(&self) -> f64 {
        self.targets
            .values()
            .map(TargetFamiliarity::strength)
            .fold(0.0, f64::max)
    }

    #[must_use]
    pub fn target_strength_for(&self, space_id: SpaceId, memory_id: MemoryId) -> f64 {
        self.familiarity(space_id, memory_id)
            .map_or(0.0, TargetFamiliarity::strength)
    }

    #[must_use]
    pub fn tag_affinity(
        &self,
        space_id: SpaceId,
        tag: &str,
        memory_id: MemoryId,
    ) -> Option<&AffinityStats> {
        self.tag_affinity
            .get(&(space_id, memory_id, tag.to_owned()))
    }

    #[must_use]
    pub fn query_class_affinity(
        &self,
        space_id: SpaceId,
        query_class: &str,
        memory_id: MemoryId,
    ) -> Option<&AffinityStats> {
        self.query_class_affinity
            .get(&(space_id, memory_id, query_class.to_owned()))
    }

    #[must_use]
    pub fn revision_familiarity(
        &self,
        space_id: SpaceId,
        memory_id: MemoryId,
        revision_id: RevisionId,
    ) -> Option<&RevisionFamiliarity> {
        self.familiarity(space_id, memory_id)
            .and_then(|target| target.revisions.get(&revision_id))
    }

    pub(crate) fn set_generation(&mut self, generation: AdaptiveGeneration) {
        self.generation = self.generation.max(generation);
    }

    pub(crate) fn target_mut(
        &mut self,
        space_id: SpaceId,
        memory_id: MemoryId,
    ) -> &mut TargetFamiliarity {
        self.targets.entry((space_id, memory_id)).or_default()
    }

    pub(crate) fn tag_affinity_mut(
        &mut self,
        space_id: SpaceId,
        memory_id: MemoryId,
        tag: &str,
    ) -> &mut AffinityStats {
        self.tag_affinity
            .entry((space_id, memory_id, tag.to_owned()))
            .or_default()
    }

    pub(crate) fn query_class_affinity_mut(
        &mut self,
        space_id: SpaceId,
        memory_id: MemoryId,
        query_class: &str,
    ) -> &mut AffinityStats {
        self.query_class_affinity
            .entry((space_id, memory_id, query_class.to_owned()))
            .or_default()
    }

    pub fn replay(events: &[crate::AdaptiveEvent]) -> Self {
        crate::reduce(events.iter().cloned())
    }

    pub fn from_checkpoint_and_tail<I>(checkpoint: crate::AdaptiveCheckpoint, tail: I) -> Self
    where
        I: IntoIterator<Item = crate::AdaptiveEvent>,
    {
        let mut state = checkpoint.state;
        state.set_generation(checkpoint.generation);
        for event in tail {
            crate::reducer::apply_event(&mut state, &event);
        }
        state
    }

    pub(crate) fn target_rows(
        &self,
    ) -> impl Iterator<Item = (&(SpaceId, MemoryId), &TargetFamiliarity)> {
        self.targets.iter()
    }

    pub(crate) fn tag_affinity_rows(
        &self,
    ) -> impl Iterator<Item = (&(SpaceId, MemoryId, String), &AffinityStats)> {
        self.tag_affinity.iter()
    }

    pub(crate) fn query_class_affinity_rows(
        &self,
    ) -> impl Iterator<Item = (&(SpaceId, MemoryId, String), &AffinityStats)> {
        self.query_class_affinity.iter()
    }
}

impl TargetFamiliarity {
    #[must_use]
    pub fn strength(&self) -> f64 {
        saturation(self.positive_weight)
    }

    #[must_use]
    pub fn accessibility_at(&self, now: Timestamp) -> f64 {
        let Some(last_success) = self.last_success_at else {
            return 0.0;
        };
        let elapsed = now
            .unix_seconds()
            .saturating_sub(last_success.unix_seconds())
            .max(0) as f64;
        self.strength() * (-elapsed / RECENCY_HALF_LIFE_SECONDS).exp()
    }

    pub(crate) fn revision_mut(&mut self, revision_id: RevisionId) -> &mut RevisionFamiliarity {
        self.revisions.entry(revision_id).or_default()
    }
}

impl AffinityStats {
    #[must_use]
    pub fn score(&self) -> f64 {
        let total = self.positive_weight + self.negative_weight;
        if total == 0.0 {
            0.0
        } else {
            (self.positive_weight - self.negative_weight) / total
        }
    }

    pub(crate) fn record_positive(&mut self, weight: f64) {
        self.positive_count = self.positive_count.saturating_add(1);
        self.positive_weight += weight;
    }

    pub(crate) fn record_negative(&mut self, weight: f64) {
        self.negative_count = self.negative_count.saturating_add(1);
        self.negative_weight += weight;
    }
}

#[must_use]
pub(crate) fn saturation(positive_weight: f64) -> f64 {
    1.0 - (-positive_weight / SATURATION_SCALE).exp()
}
