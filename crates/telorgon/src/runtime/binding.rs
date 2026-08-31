use crate::ui::{MountedUi, Property, PropertyValue, UiTransaction};

use crate::runtime::{
    ComponentId, Read, RuntimeResult, read::ReadKey, read_arena::ReadArena, state_arena::StateArena,
};

trait ErasedBinding {
    fn owner(&self) -> ComponentId;
    fn source(&self) -> ReadKey;
    fn last_revision(&self) -> u64;
    fn set_last_revision(&mut self, revision: u64);
    fn stage(
        &self,
        reads: &ReadArena,
        states: &StateArena,
        tx: &mut UiTransaction<'_>,
    ) -> RuntimeResult<u64>;
}

struct DirectBinding<T: 'static> {
    owner: ComponentId,
    source: Read<T>,
    property: Property<T>,
    last_revision: u64,
}

struct MappedBinding<T: 'static, U: 'static, F: 'static> {
    owner: ComponentId,
    source: Read<T>,
    property: Property<U>,
    map: F,
    last_revision: u64,
}

impl<T> ErasedBinding for DirectBinding<T>
where
    T: Clone + Into<PropertyValue> + 'static,
{
    fn owner(&self) -> ComponentId {
        self.owner
    }

    fn source(&self) -> ReadKey {
        self.source.key
    }

    fn last_revision(&self) -> u64 {
        self.last_revision
    }

    fn set_last_revision(&mut self, revision: u64) {
        self.last_revision = revision;
    }

    fn stage(
        &self,
        reads: &ReadArena,
        states: &StateArena,
        tx: &mut UiTransaction<'_>,
    ) -> RuntimeResult<u64> {
        let value = reads.get(self.source, states)?.clone();
        tx.set(self.property, value);
        reads.revision(self.source.key)
    }
}

impl<T, U, F> ErasedBinding for MappedBinding<T, U, F>
where
    T: 'static,
    U: Into<PropertyValue> + 'static,
    F: Fn(&T) -> U + 'static,
{
    fn owner(&self) -> ComponentId {
        self.owner
    }

    fn source(&self) -> ReadKey {
        self.source.key
    }

    fn last_revision(&self) -> u64 {
        self.last_revision
    }

    fn set_last_revision(&mut self, revision: u64) {
        self.last_revision = revision;
    }

    fn stage(
        &self,
        reads: &ReadArena,
        states: &StateArena,
        tx: &mut UiTransaction<'_>,
    ) -> RuntimeResult<u64> {
        tx.set(self.property, (self.map)(reads.get(self.source, states)?));
        reads.revision(self.source.key)
    }
}

#[derive(Default)]
pub(crate) struct BindingArena {
    bindings: Vec<Box<dyn ErasedBinding>>,
}

impl BindingArena {
    pub(crate) fn insert<T>(
        &mut self,
        owner: ComponentId,
        read: Read<T>,
        property: Property<T>,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: Clone + Into<PropertyValue> + 'static,
    {
        reads.validate_read(owner, read)?;
        self.bindings.push(Box::new(DirectBinding {
            owner,
            source: read,
            property,
            last_revision: 0,
        }));
        Ok(())
    }

    pub(crate) fn insert_map<T, U, F>(
        &mut self,
        owner: ComponentId,
        read: Read<T>,
        property: Property<U>,
        map: F,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        U: Into<PropertyValue> + 'static,
        F: Fn(&T) -> U + 'static,
    {
        reads.validate_read(owner, read)?;
        self.bindings.push(Box::new(MappedBinding {
            owner,
            source: read,
            property,
            map,
            last_revision: 0,
        }));
        Ok(())
    }

    pub(crate) fn apply_current(
        &mut self,
        reads: &mut ReadArena,
        states: &StateArena,
        ui: &mut MountedUi,
    ) -> RuntimeResult<usize> {
        let mut pending = Vec::new();
        for (index, binding) in self.bindings.iter().enumerate() {
            reads.evaluate(binding.source(), states)?;
            let revision = reads.revision(binding.source())?;
            if revision != binding.last_revision() {
                pending.push((index, revision));
            }
        }
        if pending.is_empty() {
            return Ok(0);
        }

        let mut applied = Vec::with_capacity(pending.len());
        let (staged, result) = ui.transaction(|tx| -> RuntimeResult<()> {
            for (index, _) in &pending {
                let revision = self.bindings[*index].stage(reads, states, tx)?;
                applied.push((*index, revision));
            }
            Ok(())
        });
        staged?;
        for (index, revision) in applied {
            self.bindings[index].set_last_revision(revision);
        }
        Ok(result.property_patches)
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) -> usize {
        let before = self.bindings.len();
        self.bindings.retain(|binding| binding.owner() != owner);
        before - self.bindings.len()
    }

    pub(crate) fn live(&self) -> usize {
        self.bindings.len()
    }
}
