use std::any::{Any, TypeId};

use crate::runtime::{
    ComponentId, RuntimeError, RuntimeResult, State, state::StateKey, state_arena::StateArena,
};

type EqualityFn = fn(&dyn Any, &dyn Any) -> bool;

struct StagedState {
    key: StateKey,
    type_id: TypeId,
    value: Box<dyn Any>,
    equality: Option<EqualityFn>,
}

#[derive(Debug, Default)]
pub(crate) struct TransactionCommit {
    pub(crate) changed: Vec<StateKey>,
    pub(crate) staged: u64,
    pub(crate) coalesced: u64,
    pub(crate) committed: u64,
    pub(crate) equal_suppressed: u64,
}

pub(crate) struct StateTransaction {
    owner: ComponentId,
    staged: Vec<StagedState>,
    failed: Option<RuntimeError>,
    staged_count: u64,
    coalesced: u64,
}

impl StateTransaction {
    pub(crate) fn new(owner: ComponentId) -> Self {
        Self {
            owner,
            staged: Vec::new(),
            failed: None,
            staged_count: 0,
            coalesced: 0,
        }
    }

    pub(crate) fn get<T: Clone + 'static>(
        &self,
        states: &StateArena,
        state: State<T>,
    ) -> RuntimeResult<T> {
        states.validate_type(self.owner, state.key, TypeId::of::<T>())?;
        if let Some(staged) = self.staged.iter().find(|staged| staged.key == state.key) {
            return staged
                .value
                .downcast_ref::<T>()
                .cloned()
                .ok_or_else(|| RuntimeError::new("staged state value has the wrong type"));
        }
        states.get(self.owner, state).cloned()
    }

    pub(crate) fn set<T: PartialEq + 'static>(
        &mut self,
        states: &StateArena,
        state: State<T>,
        value: T,
    ) -> RuntimeResult<()> {
        self.stage(states, state, value, Some(equal::<T>))
    }

    pub(crate) fn replace_always<T: 'static>(
        &mut self,
        states: &StateArena,
        state: State<T>,
        value: T,
    ) -> RuntimeResult<()> {
        self.stage(states, state, value, None)
    }

    fn stage<T: 'static>(
        &mut self,
        states: &StateArena,
        state: State<T>,
        value: T,
        equality: Option<EqualityFn>,
    ) -> RuntimeResult<()> {
        if let Err(error) = states.validate_type(self.owner, state.key, TypeId::of::<T>()) {
            self.failed.get_or_insert_with(|| error.clone());
            return Err(error);
        }
        self.staged_count += 1;
        if let Some(staged) = self
            .staged
            .iter_mut()
            .find(|staged| staged.key == state.key)
        {
            staged.value = Box::new(value);
            staged.equality = equality;
            self.coalesced += 1;
        } else {
            self.staged.push(StagedState {
                key: state.key,
                type_id: TypeId::of::<T>(),
                value: Box::new(value),
                equality,
            });
        }
        Ok(())
    }

    pub(crate) fn commit(self, states: &mut StateArena) -> RuntimeResult<TransactionCommit> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        for staged in &self.staged {
            states.validate_type(self.owner, staged.key, staged.type_id)?;
        }

        let mut commit = TransactionCommit {
            staged: self.staged_count,
            coalesced: self.coalesced,
            ..TransactionCommit::default()
        };
        for staged in self.staged {
            let unchanged = staged.equality.is_some_and(|equality| {
                states
                    .value_any(staged.key)
                    .is_ok_and(|current| equality(current, staged.value.as_ref()))
            });
            if unchanged {
                commit.equal_suppressed += 1;
                continue;
            }
            states.replace_any(staged.key, staged.value)?;
            commit.changed.push(staged.key);
            commit.committed += 1;
        }
        Ok(commit)
    }

    pub(crate) fn staged_value(&self, key: StateKey) -> Option<&dyn Any> {
        self.staged
            .iter()
            .find(|staged| staged.key == key)
            .map(|staged| staged.value.as_ref())
    }

    pub(crate) fn has_staged(&self, key: StateKey) -> bool {
        self.staged.iter().any(|staged| staged.key == key)
    }
}

fn equal<T: PartialEq + 'static>(left: &dyn Any, right: &dyn Any) -> bool {
    left.downcast_ref::<T>()
        .zip(right.downcast_ref::<T>())
        .is_some_and(|(left, right)| left == right)
}
