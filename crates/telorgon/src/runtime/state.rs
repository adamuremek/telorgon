use std::marker::PhantomData;
use std::rc::Rc;

use crate::runtime::{ComponentId, Read, read::ReadKey};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StateKey {
    pub(crate) view: u64,
    pub(crate) owner: ComponentId,
    pub(crate) index: u32,
    pub(crate) generation: u32,
}

/// An owner-scoped handle to a runtime-held value.
///
/// Values are not stored in this handle. The `Rc` marker deliberately makes state handles local to
/// their single-writer view runtime (`!Send` and `!Sync`).
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct State<T: 'static> {
    pub(crate) key: StateKey,
    pub(crate) read: Option<ReadKey>,
    marker: PhantomData<fn() -> T>,
    local: PhantomData<Rc<()>>,
}

impl<T: 'static> State<T> {
    pub(crate) fn new(key: StateKey) -> Self {
        Self {
            key,
            read: None,
            marker: PhantomData,
            local: PhantomData,
        }
    }

    pub fn owner(self) -> ComponentId {
        self.key.owner
    }

    pub(crate) fn with_read(mut self, read: Read<T>) -> Self {
        self.read = Some(read.key);
        self
    }

    pub fn read(self) -> Read<T> {
        Read::new(
            self.read
                .expect("State::read is available for component-created state"),
        )
    }
}

impl<T: 'static> Copy for State<T> {}

impl<T: 'static> Clone for State<T> {
    fn clone(&self) -> Self {
        *self
    }
}
