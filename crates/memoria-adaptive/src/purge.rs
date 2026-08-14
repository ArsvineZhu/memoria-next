use memoria_types::MemoryId;

use crate::AdaptiveEvent;

/// Physically rewrite the event stream without any event linked to a Memory.
///
/// This deliberately operates on the durable event representation rather than
/// only hiding a target in a materialized state, so a later rebuild cannot
/// resurrect purged adaptive evidence.
#[must_use]
pub fn rewrite_without_memory(events: &[AdaptiveEvent], memory_id: MemoryId) -> Vec<AdaptiveEvent> {
    events
        .iter()
        .filter(|event| event.memory_id != memory_id)
        .cloned()
        .collect()
}
