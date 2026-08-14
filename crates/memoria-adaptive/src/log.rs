use std::collections::BTreeMap;

use memoria_types::{AdaptiveGeneration, MemoryId, SpaceId};

use crate::{AdaptiveError, AdaptiveEvent, FeedbackEventInput};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdaptiveLogCommit {
    pub generation: AdaptiveGeneration,
    pub events: Vec<AdaptiveEvent>,
}

#[derive(Clone, Debug, Default)]
pub struct AdaptiveEventLog {
    generation: AdaptiveGeneration,
    events: Vec<AdaptiveEvent>,
    by_idempotency_key: BTreeMap<String, usize>,
    by_event_id: BTreeMap<String, usize>,
}

impl AdaptiveEventLog {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            generation: AdaptiveGeneration::initial(),
            events: Vec::new(),
            by_idempotency_key: BTreeMap::new(),
            by_event_id: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn current_generation(&self) -> AdaptiveGeneration {
        self.generation
    }

    #[must_use]
    pub fn events(&self) -> &[AdaptiveEvent] {
        &self.events
    }

    pub fn from_events<I>(events: I) -> Result<Self, AdaptiveError>
    where
        I: IntoIterator<Item = AdaptiveEvent>,
    {
        let mut log = Self::new();
        log.events = events.into_iter().collect();
        log.rebuild_indexes()?;
        Ok(log)
    }

    pub fn rewrite_without_memory(&mut self, memory_id: MemoryId) -> Result<(), AdaptiveError> {
        self.events = crate::purge::rewrite_without_memory(&self.events, memory_id);
        self.rebuild_indexes()
    }

    pub fn reset_space(&mut self, space_id: SpaceId) -> Result<(), AdaptiveError> {
        self.events = crate::reset::reset_space(&self.events, space_id);
        self.rebuild_indexes()
    }

    pub fn reset_store(&mut self) -> Result<(), AdaptiveError> {
        self.events = crate::reset::reset_store(&self.events);
        self.rebuild_indexes()
    }

    pub fn append_batch<I>(&mut self, inputs: I) -> Result<AdaptiveLogCommit, AdaptiveError>
    where
        I: IntoIterator<Item = FeedbackEventInput>,
    {
        let inputs: Vec<_> = inputs.into_iter().collect();
        for input in &inputs {
            input.validate()?;
        }

        let mut staged_by_key = BTreeMap::<String, FeedbackEventInput>::new();
        let mut staged_by_event_id = BTreeMap::<String, FeedbackEventInput>::new();

        for input in &inputs {
            if let Some(index) = self.by_idempotency_key.get(&input.idempotency_key) {
                if self.events[*index].as_input() != *input {
                    return Err(AdaptiveError::IdempotencyConflict {
                        key: input.idempotency_key.clone(),
                    });
                }
                continue;
            }
            if let Some(existing) = staged_by_key.get(&input.idempotency_key) {
                if existing != input {
                    return Err(AdaptiveError::IdempotencyConflict {
                        key: input.idempotency_key.clone(),
                    });
                }
                continue;
            }
            if self.by_event_id.contains_key(&input.event_id)
                || staged_by_event_id.contains_key(&input.event_id)
            {
                return Err(AdaptiveError::EventIdConflict {
                    event_id: input.event_id.clone(),
                });
            }
            staged_by_key.insert(input.idempotency_key.clone(), input.clone());
            staged_by_event_id.insert(input.event_id.clone(), input.clone());
        }

        if !staged_by_key.is_empty() {
            let generation = self
                .generation
                .checked_next()
                .ok_or(AdaptiveError::GenerationExhausted)?;
            for input in staged_by_key.values() {
                let event = AdaptiveEvent::from_input(input.clone(), generation);
                let index = self.events.len();
                self.by_idempotency_key
                    .insert(event.idempotency_key.clone(), index);
                self.by_event_id.insert(event.event_id.clone(), index);
                self.events.push(event);
            }
            self.generation = generation;
        }

        let events = inputs
            .iter()
            .filter_map(|input| {
                self.by_idempotency_key
                    .get(&input.idempotency_key)
                    .map(|index| self.events[*index].clone())
            })
            .collect();

        Ok(AdaptiveLogCommit {
            generation: self.generation,
            events,
        })
    }

    fn rebuild_indexes(&mut self) -> Result<(), AdaptiveError> {
        self.by_idempotency_key.clear();
        self.by_event_id.clear();
        for (index, event) in self.events.iter().enumerate() {
            event.as_input().validate()?;
            if self
                .by_event_id
                .insert(event.event_id.clone(), index)
                .is_some()
            {
                return Err(AdaptiveError::EventIdConflict {
                    event_id: event.event_id.clone(),
                });
            }
            if self
                .by_idempotency_key
                .insert(event.idempotency_key.clone(), index)
                .is_some()
            {
                return Err(AdaptiveError::IdempotencyConflict {
                    key: event.idempotency_key.clone(),
                });
            }
            self.generation = self.generation.max(event.generation);
        }
        Ok(())
    }
}
