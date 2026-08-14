use memoria_types::AdaptiveGeneration;
use serde::{Deserialize, Serialize};

use crate::{AdaptiveEvent, AdaptiveStateV1};

/// A rebuildable materialized prefix of the adaptive event stream.
///
/// The event count identifies the prefix represented by the checkpoint. The
/// state remains the source of truth for replay; the generation is retained
/// explicitly so a persisted checkpoint can be validated by its caller.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveCheckpoint {
    pub event_count: usize,
    pub generation: AdaptiveGeneration,
    pub state: AdaptiveStateV1,
}

/// Materialize a checkpoint after `event_count` events and return the
/// remaining tail for incremental replay.
#[must_use]
pub fn checkpoint_after(
    events: &[AdaptiveEvent],
    event_count: usize,
) -> (AdaptiveCheckpoint, Vec<AdaptiveEvent>) {
    let prefix_len = event_count.min(events.len());
    let state = AdaptiveStateV1::replay(&events[..prefix_len]);
    let checkpoint = AdaptiveCheckpoint {
        event_count: prefix_len,
        generation: state.generation(),
        state,
    };
    (checkpoint, events[prefix_len..].to_vec())
}
