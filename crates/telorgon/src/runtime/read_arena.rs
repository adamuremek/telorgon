use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::rc::Rc;

use crate::runtime::{
    ComponentId, Read, RuntimeError, RuntimeResult, read::ReadKey, state::StateKey,
    state_arena::StateArena, transaction::StateTransaction,
};

trait Computation {
    fn evaluation_dependencies(&self) -> Vec<ReadKey>;
    fn compute(&self, reads: &ReadArena, states: &StateArena) -> RuntimeResult<ComputedValue>;
    fn compute_preview(
        &self,
        reads: &ReadArena,
        preview: &mut ReadPreview<'_>,
    ) -> RuntimeResult<Box<dyn Any>>;
    fn equal(&self, left: &dyn Any, right: &dyn Any) -> bool;
}

enum PreviewValue<'a> {
    Borrowed(&'a dyn Any),
    Computed(Rc<dyn Any>),
}

impl PreviewValue<'_> {
    fn as_any(&self) -> &dyn Any {
        match self {
            Self::Borrowed(value) => *value,
            Self::Computed(value) => value.as_ref(),
        }
    }
}

struct ReadPreview<'a> {
    states: &'a StateArena,
    transaction: &'a StateTransaction,
    computed: HashMap<ReadKey, Rc<dyn Any>>,
    stack: Vec<ReadKey>,
}

struct ComputedValue {
    value: Box<dyn Any>,
    dependencies: Option<Vec<ReadKey>>,
}

struct MapComputation<T: 'static, U, F> {
    source: Read<T>,
    map: F,
    marker: std::marker::PhantomData<fn() -> U>,
}

impl<T, U, F> Computation for MapComputation<T, U, F>
where
    T: 'static,
    U: PartialEq + 'static,
    F: Fn(&T) -> U + 'static,
{
    fn evaluation_dependencies(&self) -> Vec<ReadKey> {
        vec![self.source.key]
    }

    fn compute(&self, reads: &ReadArena, states: &StateArena) -> RuntimeResult<ComputedValue> {
        Ok(ComputedValue {
            value: Box::new((self.map)(reads.get(self.source, states)?)),
            dependencies: None,
        })
    }

    fn compute_preview(
        &self,
        reads: &ReadArena,
        preview: &mut ReadPreview<'_>,
    ) -> RuntimeResult<Box<dyn Any>> {
        let source = reads.preview_value(self.source.key, preview)?;
        let source = source
            .as_any()
            .downcast_ref::<T>()
            .ok_or_else(|| RuntimeError::new("map preview source has the wrong type"))?;
        Ok(Box::new((self.map)(source)))
    }

    fn equal(&self, left: &dyn Any, right: &dyn Any) -> bool {
        left.downcast_ref::<U>()
            .zip(right.downcast_ref::<U>())
            .is_some_and(|(left, right)| left == right)
    }
}

struct ZipComputation<A: 'static, B: 'static, U, F> {
    left: Read<A>,
    right: Read<B>,
    map: F,
    marker: std::marker::PhantomData<fn() -> U>,
}

impl<A, B, U, F> Computation for ZipComputation<A, B, U, F>
where
    A: 'static,
    B: 'static,
    U: PartialEq + 'static,
    F: Fn(&A, &B) -> U + 'static,
{
    fn evaluation_dependencies(&self) -> Vec<ReadKey> {
        vec![self.left.key, self.right.key]
    }

    fn compute(&self, reads: &ReadArena, states: &StateArena) -> RuntimeResult<ComputedValue> {
        Ok(ComputedValue {
            value: Box::new((self.map)(
                reads.get(self.left, states)?,
                reads.get(self.right, states)?,
            )),
            dependencies: None,
        })
    }

    fn compute_preview(
        &self,
        reads: &ReadArena,
        preview: &mut ReadPreview<'_>,
    ) -> RuntimeResult<Box<dyn Any>> {
        let left = reads.preview_value(self.left.key, preview)?;
        let right = reads.preview_value(self.right.key, preview)?;
        let left = left
            .as_any()
            .downcast_ref::<A>()
            .ok_or_else(|| RuntimeError::new("zip preview left source has the wrong type"))?;
        let right = right
            .as_any()
            .downcast_ref::<B>()
            .ok_or_else(|| RuntimeError::new("zip preview right source has the wrong type"))?;
        Ok(Box::new((self.map)(left, right)))
    }

    fn equal(&self, left: &dyn Any, right: &dyn Any) -> bool {
        left.downcast_ref::<U>()
            .zip(right.downcast_ref::<U>())
            .is_some_and(|(left, right)| left == right)
    }
}

struct SelectComputation<T: 'static> {
    condition: Read<bool>,
    when_true: Read<T>,
    when_false: Read<T>,
}

impl<T> Computation for SelectComputation<T>
where
    T: Clone + PartialEq + 'static,
{
    fn evaluation_dependencies(&self) -> Vec<ReadKey> {
        vec![self.condition.key, self.when_true.key, self.when_false.key]
    }

    fn compute(&self, reads: &ReadArena, states: &StateArena) -> RuntimeResult<ComputedValue> {
        let active = if *reads.get(self.condition, states)? {
            self.when_true
        } else {
            self.when_false
        };
        Ok(ComputedValue {
            value: Box::new(reads.get(active, states)?.clone()),
            dependencies: Some(vec![self.condition.key, active.key]),
        })
    }

    fn compute_preview(
        &self,
        reads: &ReadArena,
        preview: &mut ReadPreview<'_>,
    ) -> RuntimeResult<Box<dyn Any>> {
        let condition = reads.preview_value(self.condition.key, preview)?;
        let condition = condition
            .as_any()
            .downcast_ref::<bool>()
            .copied()
            .ok_or_else(|| RuntimeError::new("select preview condition has the wrong type"))?;
        let active = if condition {
            self.when_true
        } else {
            self.when_false
        };
        let value = reads.preview_value(active.key, preview)?;
        let value = value
            .as_any()
            .downcast_ref::<T>()
            .ok_or_else(|| RuntimeError::new("select preview branch has the wrong type"))?;
        Ok(Box::new(value.clone()))
    }

    fn equal(&self, left: &dyn Any, right: &dyn Any) -> bool {
        left.downcast_ref::<T>()
            .zip(right.downcast_ref::<T>())
            .is_some_and(|(left, right)| left == right)
    }
}

enum ReadKind {
    Source(StateKey),
    Derived {
        dependencies: Vec<ReadKey>,
        computation: Option<Box<dyn Computation>>,
    },
}

struct ReadSlot {
    generation: u32,
    owner: ComponentId,
    type_id: TypeId,
    kind: ReadKind,
    value: Option<Box<dyn Any>>,
    revision: u64,
    dirty: bool,
    evaluating: bool,
    dependents: Vec<ReadKey>,
}

pub(crate) struct ReadArena {
    view: u64,
    slots: Vec<Option<ReadSlot>>,
    state_sources: Vec<(StateKey, ReadKey)>,
    work: Vec<ReadKey>,
    evaluation_stack: Vec<ReadKey>,
    pub(crate) evaluated: u64,
    pub(crate) unchanged: u64,
    pub(crate) cycles: u64,
}

impl ReadArena {
    pub(crate) fn new(view: u64) -> Self {
        Self {
            view,
            slots: Vec::new(),
            state_sources: Vec::new(),
            work: Vec::new(),
            evaluation_stack: Vec::new(),
            evaluated: 0,
            unchanged: 0,
            cycles: 0,
        }
    }

    pub(crate) fn insert_source<T: 'static>(
        &mut self,
        owner: ComponentId,
        state: StateKey,
    ) -> Read<T> {
        let key = self.push(owner, TypeId::of::<T>(), ReadKind::Source(state));
        self.state_sources.push((state, key));
        Read::new(key)
    }

    pub(crate) fn map<T, U, F>(
        &mut self,
        owner: ComponentId,
        source: Read<T>,
        map: F,
    ) -> RuntimeResult<Read<U>>
    where
        T: 'static,
        U: PartialEq + 'static,
        F: Fn(&T) -> U + 'static,
    {
        self.validate_type(source.key, TypeId::of::<T>())?;
        let key = self.push(
            owner,
            TypeId::of::<U>(),
            ReadKind::Derived {
                dependencies: vec![source.key],
                computation: Some(Box::new(MapComputation {
                    source,
                    map,
                    marker: std::marker::PhantomData,
                })),
            },
        );
        self.slot_mut(source.key)?.dependents.push(key);
        Ok(Read::new(key))
    }

    pub(crate) fn zip<A, B, U, F>(
        &mut self,
        owner: ComponentId,
        left: Read<A>,
        right: Read<B>,
        map: F,
    ) -> RuntimeResult<Read<U>>
    where
        A: 'static,
        B: 'static,
        U: PartialEq + 'static,
        F: Fn(&A, &B) -> U + 'static,
    {
        self.validate_type(left.key, TypeId::of::<A>())?;
        self.validate_type(right.key, TypeId::of::<B>())?;
        let key = self.push(
            owner,
            TypeId::of::<U>(),
            ReadKind::Derived {
                dependencies: vec![left.key, right.key],
                computation: Some(Box::new(ZipComputation {
                    left,
                    right,
                    map,
                    marker: std::marker::PhantomData,
                })),
            },
        );
        self.slot_mut(left.key)?.dependents.push(key);
        self.slot_mut(right.key)?.dependents.push(key);
        Ok(Read::new(key))
    }

    pub(crate) fn select<T>(
        &mut self,
        owner: ComponentId,
        condition: Read<bool>,
        when_true: Read<T>,
        when_false: Read<T>,
    ) -> RuntimeResult<Read<T>>
    where
        T: Clone + PartialEq + 'static,
    {
        self.validate_type(condition.key, TypeId::of::<bool>())?;
        self.validate_type(when_true.key, TypeId::of::<T>())?;
        self.validate_type(when_false.key, TypeId::of::<T>())?;
        let dependencies = vec![condition.key, when_true.key, when_false.key];
        let key = self.push(
            owner,
            TypeId::of::<T>(),
            ReadKind::Derived {
                dependencies: dependencies.clone(),
                computation: Some(Box::new(SelectComputation {
                    condition,
                    when_true,
                    when_false,
                })),
            },
        );
        for dependency in dependencies {
            self.slot_mut(dependency)?.dependents.push(key);
        }
        Ok(Read::new(key))
    }

    pub(crate) fn invalidate_states(&mut self, changed: &[StateKey]) {
        self.work.clear();
        for state in changed {
            if let Some((_, source)) = self.state_sources.iter().find(|(key, _)| key == state) {
                self.work.push(*source);
            }
        }
        while let Some(key) = self.work.pop() {
            let dependents = match self.slot_mut(key) {
                Ok(slot) => {
                    slot.dirty = true;
                    slot.dependents.clone()
                }
                Err(_) => continue,
            };
            self.work.extend(dependents);
        }
    }

    pub(crate) fn evaluate(&mut self, key: ReadKey, states: &StateArena) -> RuntimeResult<()> {
        if !self.slot(key)?.dirty {
            return Ok(());
        }
        if self.slot(key)?.evaluating {
            self.cycles += 1;
            let mut path = self
                .evaluation_stack
                .iter()
                .map(|read| read.index.to_string())
                .collect::<Vec<_>>();
            path.push(key.index.to_string());
            return Err(RuntimeError::new(format!(
                "read dependency cycle at component {:?}, read slots {}",
                key.owner,
                path.join(" -> ")
            )));
        }
        self.slot_mut(key)?.evaluating = true;
        self.evaluation_stack.push(key);
        let dependencies = match &self.slot(key)?.kind {
            ReadKind::Source(_) => Vec::new(),
            ReadKind::Derived {
                dependencies,
                computation,
            } => {
                let mut evaluation = dependencies.clone();
                for dependency in computation
                    .as_deref()
                    .ok_or_else(|| RuntimeError::new("read evaluation is reentrant"))?
                    .evaluation_dependencies()
                {
                    if !evaluation.contains(&dependency) {
                        evaluation.push(dependency);
                    }
                }
                evaluation
            }
        };
        for dependency in dependencies {
            if let Err(error) = self.evaluate(dependency, states) {
                self.slot_mut(key)?.evaluating = false;
                self.evaluation_stack.pop();
                return Err(error);
            }
        }

        let source = match self.slot(key)?.kind {
            ReadKind::Source(state) => Some(state),
            ReadKind::Derived { .. } => None,
        };
        if let Some(state) = source {
            let revision = states.revision(state)?;
            let slot = self.slot_mut(key)?;
            slot.revision = revision;
            slot.dirty = false;
            slot.evaluating = false;
            self.evaluation_stack.pop();
            return Ok(());
        }

        let computation = match &mut self.slot_mut(key)?.kind {
            ReadKind::Derived { computation, .. } => computation
                .take()
                .ok_or_else(|| RuntimeError::new("read evaluation is reentrant"))?,
            ReadKind::Source(_) => unreachable!(),
        };
        let result = computation.compute(self, states);
        let (unchanged, replacement) = {
            let slot = self.slot_mut(key)?;
            if let ReadKind::Derived {
                computation: stored,
                ..
            } = &mut slot.kind
            {
                *stored = Some(computation);
            }
            let computed = match result {
                Ok(computed) => computed,
                Err(error) => {
                    slot.evaluating = false;
                    self.evaluation_stack.pop();
                    return Err(error);
                }
            };
            let value = computed.value;
            let unchanged = slot.value.as_deref().is_some_and(|old| {
                stored_computation(slot).is_some_and(|item| item.equal(old, &*value))
            });
            if !unchanged {
                slot.value = Some(value);
                slot.revision = slot.revision.wrapping_add(1).max(1);
            }
            slot.dirty = false;
            slot.evaluating = false;
            (unchanged, computed.dependencies)
        };
        self.evaluation_stack.pop();
        if let Some(dependencies) = replacement {
            self.replace_dependencies(key, dependencies)?;
        }
        if unchanged {
            self.unchanged += 1;
        }
        self.evaluated += 1;
        Ok(())
    }

    pub(crate) fn get<'a, T: 'static>(
        &'a self,
        read: Read<T>,
        states: &'a StateArena,
    ) -> RuntimeResult<&'a T> {
        let slot = self.validate(read.key.owner, read.key, TypeId::of::<T>())?;
        match slot.kind {
            ReadKind::Source(state) => states.get_key::<T>(state),
            ReadKind::Derived { .. } => slot
                .value
                .as_deref()
                .and_then(|value| value.downcast_ref::<T>())
                .ok_or_else(|| RuntimeError::new("derived read has no evaluated value")),
        }
    }

    pub(crate) fn revision(&self, key: ReadKey) -> RuntimeResult<u64> {
        Ok(self.slot(key)?.revision)
    }

    /// Evaluates one read against the transaction's staged source overlay without changing live
    /// read values, revisions, dependencies, dirty flags, or diagnostics. Unaffected reads skip the
    /// preview and its validator.
    pub(crate) fn validate_staged<T, F>(
        &self,
        read: Read<T>,
        states: &StateArena,
        transaction: &StateTransaction,
        validate: F,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        F: FnOnce(&T) -> RuntimeResult<()>,
    {
        self.validate_type(read.key, TypeId::of::<T>())?;
        if !self.transaction_affects(read.key, transaction, &mut HashMap::new(), &mut Vec::new())? {
            return Ok(());
        }
        let mut preview = ReadPreview {
            states,
            transaction,
            computed: HashMap::new(),
            stack: Vec::new(),
        };
        let value = self.preview_value(read.key, &mut preview)?;
        let value = value
            .as_any()
            .downcast_ref::<T>()
            .ok_or_else(|| RuntimeError::new("read preview produced the wrong value type"))?;
        validate(value)
    }

    pub(crate) fn validate_read<T: 'static>(
        &self,
        _owner: ComponentId,
        read: Read<T>,
    ) -> RuntimeResult<()> {
        self.validate_type(read.key, TypeId::of::<T>()).map(|_| ())
    }

    pub(crate) fn remove(&mut self, key: ReadKey) -> RuntimeResult<()> {
        let dependencies = match &self.slot(key)?.kind {
            ReadKind::Source(_) => Vec::new(),
            ReadKind::Derived { dependencies, .. } => dependencies.clone(),
        };
        for dependency in dependencies {
            self.slot_mut(dependency)?
                .dependents
                .retain(|dependent| *dependent != key);
        }
        self.state_sources.retain(|(_, source)| *source != key);
        self.slots[key.index as usize] = None;
        Ok(())
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) {
        for slot in &mut self.slots {
            if slot.as_ref().is_some_and(|slot| slot.owner == owner) {
                *slot = None;
            }
        }
        self.state_sources.retain(|(_, read)| read.owner != owner);
    }

    pub(crate) fn live(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    pub(crate) fn dependency_count(&self) -> usize {
        self.slots
            .iter()
            .filter_map(Option::as_ref)
            .map(|slot| match &slot.kind {
                ReadKind::Source(_) => 0,
                ReadKind::Derived { dependencies, .. } => dependencies.len(),
            })
            .sum()
    }

    fn replace_dependencies(
        &mut self,
        key: ReadKey,
        dependencies: Vec<ReadKey>,
    ) -> RuntimeResult<()> {
        let old = match &mut self.slot_mut(key)?.kind {
            ReadKind::Derived {
                dependencies: stored,
                ..
            } => std::mem::replace(stored, dependencies.clone()),
            ReadKind::Source(_) => return Err(RuntimeError::new("source read has dependencies")),
        };
        for dependency in old {
            if !dependencies.contains(&dependency) {
                self.slot_mut(dependency)?
                    .dependents
                    .retain(|dependent| *dependent != key);
            }
        }
        for dependency in dependencies {
            let dependents = &mut self.slot_mut(dependency)?.dependents;
            if !dependents.contains(&key) {
                dependents.push(key);
            }
        }
        Ok(())
    }

    fn transaction_affects(
        &self,
        key: ReadKey,
        transaction: &StateTransaction,
        cached: &mut HashMap<ReadKey, bool>,
        stack: &mut Vec<ReadKey>,
    ) -> RuntimeResult<bool> {
        if let Some(affected) = cached.get(&key) {
            return Ok(*affected);
        }
        if stack.contains(&key) {
            return Err(RuntimeError::new(
                "read dependency cycle while validating staged structure",
            ));
        }
        stack.push(key);
        let affected = match &self.slot(key)?.kind {
            ReadKind::Source(state) => transaction.has_staged(*state),
            ReadKind::Derived {
                dependencies,
                computation,
            } => {
                let mut inputs = dependencies.clone();
                for dependency in computation
                    .as_deref()
                    .ok_or_else(|| RuntimeError::new("read preview is reentrant"))?
                    .evaluation_dependencies()
                {
                    if !inputs.contains(&dependency) {
                        inputs.push(dependency);
                    }
                }
                let mut affected = false;
                for dependency in inputs {
                    if self.transaction_affects(dependency, transaction, cached, stack)? {
                        affected = true;
                        break;
                    }
                }
                affected
            }
        };
        stack.pop();
        cached.insert(key, affected);
        Ok(affected)
    }

    fn preview_value<'a>(
        &self,
        key: ReadKey,
        preview: &mut ReadPreview<'a>,
    ) -> RuntimeResult<PreviewValue<'a>> {
        let slot = self.slot(key)?;
        if let ReadKind::Source(state) = slot.kind {
            let value = preview
                .transaction
                .staged_value(state)
                .map(Ok)
                .unwrap_or_else(|| preview.states.value_any(state))?;
            return Ok(PreviewValue::Borrowed(value));
        }
        if let Some(value) = preview.computed.get(&key) {
            return Ok(PreviewValue::Computed(value.clone()));
        }
        if preview.stack.contains(&key) {
            return Err(RuntimeError::new(
                "read dependency cycle while previewing staged structure",
            ));
        }
        preview.stack.push(key);
        let result = stored_computation(slot)
            .ok_or_else(|| RuntimeError::new("read preview is reentrant"))?
            .compute_preview(self, preview);
        preview.stack.pop();
        let value: Rc<dyn Any> = Rc::from(result?);
        preview.computed.insert(key, value.clone());
        Ok(PreviewValue::Computed(value))
    }

    fn push(&mut self, owner: ComponentId, type_id: TypeId, kind: ReadKind) -> ReadKey {
        let index = self.slots.len() as u32;
        self.slots.push(Some(ReadSlot {
            generation: 1,
            owner,
            type_id,
            kind,
            value: None,
            revision: 0,
            dirty: true,
            evaluating: false,
            dependents: Vec::new(),
        }));
        ReadKey {
            view: self.view,
            owner,
            index,
            generation: 1,
        }
    }

    fn validate(
        &self,
        owner: ComponentId,
        key: ReadKey,
        type_id: TypeId,
    ) -> RuntimeResult<&ReadSlot> {
        if key.owner != owner {
            return Err(RuntimeError::new("read belongs to another component owner"));
        }
        let slot = self.slot(key)?;
        if slot.type_id != type_id {
            return Err(RuntimeError::new("read handle has the wrong value type"));
        }
        Ok(slot)
    }

    fn validate_type(&self, key: ReadKey, type_id: TypeId) -> RuntimeResult<&ReadSlot> {
        let slot = self.slot(key)?;
        if slot.type_id != type_id {
            return Err(RuntimeError::new("read handle has the wrong value type"));
        }
        Ok(slot)
    }

    fn slot(&self, key: ReadKey) -> RuntimeResult<&ReadSlot> {
        if key.view != self.view {
            return Err(RuntimeError::new("read handle belongs to another view"));
        }
        self.slots
            .get(key.index as usize)
            .and_then(Option::as_ref)
            .filter(|slot| slot.generation == key.generation && slot.owner == key.owner)
            .ok_or_else(|| RuntimeError::new("read handle is stale"))
    }

    fn slot_mut(&mut self, key: ReadKey) -> RuntimeResult<&mut ReadSlot> {
        if key.view != self.view {
            return Err(RuntimeError::new("read handle belongs to another view"));
        }
        self.slots
            .get_mut(key.index as usize)
            .and_then(Option::as_mut)
            .filter(|slot| slot.generation == key.generation && slot.owner == key.owner)
            .ok_or_else(|| RuntimeError::new("read handle is stale"))
    }
}

fn stored_computation(slot: &ReadSlot) -> Option<&dyn Computation> {
    match &slot.kind {
        ReadKind::Derived { computation, .. } => computation.as_deref(),
        ReadKind::Source(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> ComponentId {
        ComponentId {
            view: 11,
            index: 0,
            generation: 1,
        }
    }

    #[test]
    fn diamond_dependencies_evaluate_on_demand_and_suppress_unchanged_outputs() {
        let owner = owner();
        let mut states = StateArena::new(11);
        let state = states.insert(owner, 2_i32);
        let mut reads = ReadArena::new(11);
        let source = reads.insert_source::<i32>(owner, state.key);
        let left = reads.map(owner, source, |value| value * 2).unwrap();
        let right = reads.map(owner, source, |value| value + 1).unwrap();
        let sum = reads
            .zip(owner, left, right, |left, right| left + right)
            .unwrap();
        let parity = reads.map(owner, source, |value| value % 2).unwrap();

        reads.evaluate(sum.key, &states).unwrap();
        reads.evaluate(parity.key, &states).unwrap();
        assert_eq!(reads.get(sum, &states), Ok(&7));
        let parity_revision = reads.revision(parity.key).unwrap();

        states.replace_any(state.key, Box::new(4_i32)).unwrap();
        reads.invalidate_states(&[state.key]);
        reads.evaluate(sum.key, &states).unwrap();
        reads.evaluate(parity.key, &states).unwrap();
        assert_eq!(reads.get(sum, &states), Ok(&13));
        assert_eq!(reads.revision(parity.key).unwrap(), parity_revision);
        assert!(reads.unchanged >= 1);
    }

    #[test]
    fn staged_preview_evaluates_derived_values_without_mutating_live_reads() {
        let owner = owner();
        let mut states = StateArena::new(11);
        let left_state = states.insert(owner, 2_i32);
        let right_state = states.insert(owner, 3_i32);
        let mut reads = ReadArena::new(11);
        let left = reads.insert_source::<i32>(owner, left_state.key);
        let right = reads.insert_source::<i32>(owner, right_state.key);
        let doubled = reads.map(owner, left, |value| value * 2).unwrap();
        let sum = reads
            .zip(owner, doubled, right, |left, right| left + right)
            .unwrap();
        reads.evaluate(sum.key, &states).unwrap();
        assert_eq!(reads.get(sum, &states), Ok(&7));
        let revision = reads.revision(sum.key).unwrap();
        let evaluated = reads.evaluated;

        let mut transaction = StateTransaction::new(owner);
        transaction.set(&states, left_state, 5).unwrap();
        reads
            .validate_staged(sum, &states, &transaction, |value| {
                assert_eq!(*value, 13);
                Ok(())
            })
            .unwrap();

        assert_eq!(reads.get(sum, &states), Ok(&7));
        assert_eq!(reads.revision(sum.key), Ok(revision));
        assert_eq!(reads.evaluated, evaluated);
        assert_eq!(states.get(owner, left_state), Ok(&2));
    }

    #[test]
    fn dependency_cycle_returns_a_structured_error() {
        let owner = owner();
        let mut states = StateArena::new(11);
        let state = states.insert(owner, 1_i32);
        let mut reads = ReadArena::new(11);
        let source = reads.insert_source::<i32>(owner, state.key);
        let first = reads.map(owner, source, |value| value + 1).unwrap();
        let second = reads.map(owner, first, |value| value + 1).unwrap();
        if let ReadKind::Derived { dependencies, .. } = &mut reads.slot_mut(first.key).unwrap().kind
        {
            dependencies.push(second.key);
        }
        let error = reads.evaluate(first.key, &states).unwrap_err();
        assert!(error.to_string().contains("1 -> 2 -> 1"));
        assert_eq!(reads.cycles, 1);
        assert!(reads.evaluation_stack.is_empty());
        assert!(!reads.slot(first.key).unwrap().evaluating);
    }

    #[test]
    fn select_replaces_the_inactive_dependency_after_evaluation() {
        let owner = owner();
        let mut states = StateArena::new(11);
        let condition_state = states.insert(owner, true);
        let true_state = states.insert(owner, 10_i32);
        let false_state = states.insert(owner, 20_i32);
        let mut reads = ReadArena::new(11);
        let condition = reads.insert_source::<bool>(owner, condition_state.key);
        let when_true = reads.insert_source::<i32>(owner, true_state.key);
        let when_false = reads.insert_source::<i32>(owner, false_state.key);
        let selected = reads
            .select(owner, condition, when_true, when_false)
            .unwrap();

        reads.evaluate(selected.key, &states).unwrap();
        assert_eq!(reads.get(selected, &states), Ok(&10));
        assert_eq!(reads.dependency_count(), 2);

        states
            .replace_any(false_state.key, Box::new(21_i32))
            .unwrap();
        reads.invalidate_states(&[false_state.key]);
        assert!(!reads.slot(selected.key).unwrap().dirty);

        states
            .replace_any(condition_state.key, Box::new(false))
            .unwrap();
        reads.invalidate_states(&[condition_state.key]);
        reads.evaluate(selected.key, &states).unwrap();
        assert_eq!(reads.get(selected, &states), Ok(&21));
        assert!(
            !reads
                .slot(when_true.key)
                .unwrap()
                .dependents
                .contains(&selected.key)
        );
        assert!(
            reads
                .slot(when_false.key)
                .unwrap()
                .dependents
                .contains(&selected.key)
        );
    }

    #[test]
    fn deep_invalidation_uses_bounded_stack_space() {
        let owner = owner();
        let mut states = StateArena::new(11);
        let state = states.insert(owner, 1_i32);
        let mut reads = ReadArena::new(11);
        let source = reads.insert_source::<i32>(owner, state.key);
        let mut tail = source;
        for _ in 0..10_000 {
            tail = reads.map(owner, tail, |value| *value).unwrap();
        }
        for slot in reads.slots.iter_mut().filter_map(Option::as_mut) {
            slot.dirty = false;
        }

        reads.invalidate_states(&[state.key]);

        assert!(reads.slot(tail.key).unwrap().dirty);
        assert!(reads.work.is_empty());
    }
}
