use crate::{AdaptiveEvent, AdaptiveStateV1, FeedbackOutcome};

const POSITIVE_WEIGHT: f64 = 1.0;
const PARTIAL_POSITIVE_WEIGHT: f64 = 0.5;
const NEGATIVE_WEIGHT: f64 = 1.0;

pub fn reduce<I>(events: I) -> AdaptiveStateV1
where
    I: IntoIterator<Item = AdaptiveEvent>,
{
    let mut state = AdaptiveStateV1::default();
    for event in events {
        apply_event(&mut state, &event);
    }
    state
}

pub(crate) fn apply_event(state: &mut AdaptiveStateV1, event: &AdaptiveEvent) {
    state.set_generation(event.generation);
    let (positive, weight) = match event.outcome {
        FeedbackOutcome::Used
        | FeedbackOutcome::CorrectForQuery
        | FeedbackOutcome::PreferredOver => (true, POSITIVE_WEIGHT),
        FeedbackOutcome::Sufficient => (true, PARTIAL_POSITIVE_WEIGHT),
        FeedbackOutcome::Rejected
        | FeedbackOutcome::IncorrectForQuery
        | FeedbackOutcome::Insufficient => (false, NEGATIVE_WEIGHT),
    };

    let target = state.target_mut(event.space_id, event.memory_id);
    target.last_feedback_generation = target.last_feedback_generation.max(event.generation);
    if positive {
        target.success_count = target.success_count.saturating_add(1);
        target.positive_weight += weight;
        target.last_success_at = Some(
            target
                .last_success_at
                .map_or(event.occurred_at, |previous| {
                    previous.max(event.occurred_at)
                }),
        );
        let revision = target.revision_mut(event.revision_id);
        revision.success_count = revision.success_count.saturating_add(1);
        revision.last_success_at = Some(
            revision
                .last_success_at
                .map_or(event.occurred_at, |previous| {
                    previous.max(event.occurred_at)
                }),
        );
    } else {
        target.negative_count = target.negative_count.saturating_add(1);
    }

    for tag in &event.query_signature.explicit_tags {
        let affinity = state.tag_affinity_mut(event.space_id, event.memory_id, tag);
        if positive {
            affinity.record_positive(weight);
        } else {
            affinity.record_negative(NEGATIVE_WEIGHT);
        }
    }
    if let Some(query_class) = event.query_signature.query_class.as_deref() {
        let affinity = state.query_class_affinity_mut(event.space_id, event.memory_id, query_class);
        if positive {
            affinity.record_positive(weight);
        } else {
            affinity.record_negative(NEGATIVE_WEIGHT);
        }
    }
}
