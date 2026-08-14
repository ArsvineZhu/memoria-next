use memoria_types::SpaceId;

use crate::AdaptiveEvent;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetScope {
    Store,
    Space(SpaceId),
}

/// Remove adaptive history for one Space while preserving all other history.
#[must_use]
pub fn reset_space(events: &[AdaptiveEvent], space_id: SpaceId) -> Vec<AdaptiveEvent> {
    events
        .iter()
        .filter(|event| event.space_id != space_id)
        .cloned()
        .collect()
}

/// Remove all adaptive history from the store.
#[must_use]
pub fn reset_store(_events: &[AdaptiveEvent]) -> Vec<AdaptiveEvent> {
    Vec::new()
}

/// Apply an explicit reset scope to an adaptive event stream.
#[must_use]
pub fn reset(events: &[AdaptiveEvent], scope: ResetScope) -> Vec<AdaptiveEvent> {
    match scope {
        ResetScope::Store => reset_store(events),
        ResetScope::Space(space_id) => reset_space(events, space_id),
    }
}
