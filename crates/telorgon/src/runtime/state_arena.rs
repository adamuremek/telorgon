use std::any::{Any, TypeId};

use crate::runtime::{ComponentId, RuntimeError, RuntimeResult, State, state::StateKey};

struct StateSlot {
    generation: u32,
    owner: Option<ComponentId>,
    type_id: Option<TypeId>,
    value: Option<Box<dyn Any>>,
    revision: u64,
}

pub(crate) struct StateArena {
    view: u64,
    slots: Vec<StateSlot>,
    free: Vec<u32>,
    live: usize,
}

impl StateArena {
    pub(crate) fn new(view: u64) -> Self {
        Self {
            view,
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    pub(crate) fn insert<T: 'static>(&mut self, owner: ComponentId, value: T) -> State<T> {
        let (index, generation) = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.owner = Some(owner);
            slot.type_id = Some(TypeId::of::<T>());
            slot.value = Some(Box::new(value));
            slot.revision = 1;
            (index, slot.generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(StateSlot {
                generation: 1,
                owner: Some(owner),
                type_id: Some(TypeId::of::<T>()),
                value: Some(Box::new(value)),
                revision: 1,
            });
            (index, 1)
        };
        self.live += 1;
        State::new(StateKey {
            view: self.view,
            owner,
            index,
            generation,
        })
    }

    pub(crate) fn get<T: 'static>(&self, owner: ComponentId, state: State<T>) -> RuntimeResult<&T> {
        self.validate(owner, state.key, TypeId::of::<T>())?
            .value
            .as_ref()
            .and_then(|value| value.downcast_ref::<T>())
            .ok_or_else(|| RuntimeError::new("state value type does not match its handle"))
    }

    pub(crate) fn get_key<T: 'static>(&self, key: StateKey) -> RuntimeResult<&T> {
        self.validate(key.owner, key, TypeId::of::<T>())?
            .value
            .as_ref()
            .and_then(|value| value.downcast_ref::<T>())
            .ok_or_else(|| RuntimeError::new("state value type does not match its handle"))
    }

    pub(crate) fn revision(&self, key: StateKey) -> RuntimeResult<u64> {
        Ok(self.validate_key(key)?.revision)
    }

    pub(crate) fn validate_type(
        &self,
        owner: ComponentId,
        key: StateKey,
        type_id: TypeId,
    ) -> RuntimeResult<()> {
        self.validate(owner, key, type_id).map(|_| ())
    }

    pub(crate) fn value_any(&self, key: StateKey) -> RuntimeResult<&dyn Any> {
        self.validate_key(key)?
            .value
            .as_deref()
            .ok_or_else(|| RuntimeError::new("state slot has no live value"))
    }

    pub(crate) fn replace_any(&mut self, key: StateKey, value: Box<dyn Any>) -> RuntimeResult<()> {
        let slot = self.validate_key_mut(key)?;
        if slot.type_id != Some(value.as_ref().type_id()) {
            return Err(RuntimeError::new("staged state value has the wrong type"));
        }
        slot.value = Some(value);
        slot.revision = slot.revision.wrapping_add(1).max(1);
        Ok(())
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) -> usize {
        let mut removed = 0;
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if slot.owner == Some(owner) {
                slot.owner = None;
                slot.type_id = None;
                slot.value = None;
                slot.revision = 0;
                slot.generation = slot.generation.wrapping_add(1).max(1);
                self.free.push(index as u32);
                removed += 1;
            }
        }
        self.live -= removed;
        removed
    }

    pub(crate) fn remove(&mut self, key: StateKey) -> RuntimeResult<()> {
        let slot = self.validate_key_mut(key)?;
        slot.owner = None;
        slot.type_id = None;
        slot.value = None;
        slot.revision = 0;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(key.index);
        self.live -= 1;
        Ok(())
    }

    pub(crate) fn live(&self) -> usize {
        self.live
    }

    fn validate(
        &self,
        owner: ComponentId,
        key: StateKey,
        type_id: TypeId,
    ) -> RuntimeResult<&StateSlot> {
        if key.owner != owner {
            return Err(RuntimeError::new(
                "state write/read was attempted by a non-owner component",
            ));
        }
        let slot = self.validate_key(key)?;
        if slot.type_id != Some(type_id) {
            return Err(RuntimeError::new("state handle has the wrong value type"));
        }
        Ok(slot)
    }

    fn validate_key(&self, key: StateKey) -> RuntimeResult<&StateSlot> {
        if key.view != self.view {
            return Err(RuntimeError::new("state handle belongs to another view"));
        }
        self.slots
            .get(key.index as usize)
            .filter(|slot| slot.generation == key.generation && slot.owner == Some(key.owner))
            .ok_or_else(|| RuntimeError::new("state handle is stale"))
    }

    fn validate_key_mut(&mut self, key: StateKey) -> RuntimeResult<&mut StateSlot> {
        if key.view != self.view {
            return Err(RuntimeError::new("state handle belongs to another view"));
        }
        self.slots
            .get_mut(key.index as usize)
            .filter(|slot| slot.generation == key.generation && slot.owner == Some(key.owner))
            .ok_or_else(|| RuntimeError::new("state handle is stale"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner(view: u64, index: u32) -> ComponentId {
        ComponentId {
            view,
            index,
            generation: 1,
        }
    }

    #[test]
    fn state_handles_reject_wrong_owner_view_type_and_recycled_generation() {
        let first_owner = owner(7, 1);
        let other_owner = owner(7, 2);
        let mut states = StateArena::new(7);
        let state = states.insert(first_owner, 42_u32);
        assert_eq!(states.get(first_owner, state), Ok(&42));
        assert!(states.get(other_owner, state).is_err());

        let wrong_type = State::<String>::new(state.key);
        assert!(states.get(first_owner, wrong_type).is_err());
        let other_view = StateArena::new(8);
        assert!(other_view.get(first_owner, state).is_err());

        states.remove_owner(first_owner);
        let replacement = states.insert(first_owner, 9_u32);
        assert_ne!(state.key.generation, replacement.key.generation);
        assert!(states.get(first_owner, state).is_err());
        assert_eq!(states.get(first_owner, replacement), Ok(&9));
    }
}
