use std::collections::BTreeMap;

use memoria_types::MemoryId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PurgeState {
    Planned,
    Committed,
    Cleaning,
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PurgePlan {
    pub id: String,
    pub memory_id: MemoryId,
    pub state: PurgeState,
}

#[derive(Clone, Debug, Default)]
pub struct PurgeCoordinator {
    next_id: u64,
    plans: BTreeMap<String, PurgePlan>,
}

impl PurgeCoordinator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_id: 0,
            plans: BTreeMap::new(),
        }
    }

    pub fn plan(&mut self, memory_id: MemoryId) -> PurgePlan {
        self.next_id = self.next_id.saturating_add(1);
        let plan = PurgePlan {
            id: format!("PURGE_{}", self.next_id),
            memory_id,
            state: PurgeState::Planned,
        };
        self.plans.insert(plan.id.clone(), plan.clone());
        plan
    }

    pub fn state(&self, plan_id: &str) -> Option<PurgeState> {
        self.plans.get(plan_id).map(|plan| plan.state)
    }

    pub fn plan_for(&self, plan_id: &str) -> Option<&PurgePlan> {
        self.plans.get(plan_id)
    }

    pub fn transition(
        &mut self,
        plan_id: &str,
        expected: PurgeState,
        next: PurgeState,
    ) -> Result<PurgePlan, PurgeTransitionError> {
        let plan = self
            .plans
            .get_mut(plan_id)
            .ok_or_else(|| PurgeTransitionError::NotFound {
                plan_id: plan_id.to_owned(),
            })?;
        if plan.state != expected {
            return Err(PurgeTransitionError::UnexpectedState {
                plan_id: plan_id.to_owned(),
                expected,
                actual: plan.state,
            });
        }
        plan.state = next;
        Ok(plan.clone())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PurgeTransitionError {
    NotFound {
        plan_id: String,
    },
    UnexpectedState {
        plan_id: String,
        expected: PurgeState,
        actual: PurgeState,
    },
}

#[cfg(test)]
mod tests {
    use super::{PurgeCoordinator, PurgeState};
    use memoria_types::MemoryId;

    #[test]
    fn purge_state_machine_is_ordered_and_recoverable() {
        let mut coordinator = PurgeCoordinator::new();
        let plan = coordinator.plan(MemoryId::from_bytes([1; 16]));
        coordinator
            .transition(&plan.id, PurgeState::Planned, PurgeState::Committed)
            .unwrap();
        coordinator
            .transition(&plan.id, PurgeState::Committed, PurgeState::Cleaning)
            .unwrap();
        coordinator
            .transition(&plan.id, PurgeState::Cleaning, PurgeState::Completed)
            .unwrap();
        assert_eq!(coordinator.state(&plan.id), Some(PurgeState::Completed));
    }
}
