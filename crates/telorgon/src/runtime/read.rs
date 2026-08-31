use std::marker::PhantomData;
use std::rc::Rc;

use crate::runtime::ComponentId;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ReadKey {
    pub(crate) view: u64,
    pub(crate) owner: ComponentId,
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

/// A local read-only source or derived value handle.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Read<T: 'static> {
    pub(crate) key: ReadKey,
    marker: PhantomData<fn() -> T>,
    local: PhantomData<Rc<()>>,
}

impl<T: 'static> Read<T> {
    pub(crate) fn new(key: ReadKey) -> Self {
        Self {
            key,
            marker: PhantomData,
            local: PhantomData,
        }
    }

    pub fn owner(self) -> ComponentId {
        self.key.owner
    }
}

impl<T: 'static> Copy for Read<T> {}
impl<T: 'static> Clone for Read<T> {
    fn clone(&self) -> Self {
        *self
    }
}
