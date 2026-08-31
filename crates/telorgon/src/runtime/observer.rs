use crate::runtime::{
    ComponentId, Read, RuntimeResult,
    read::ReadKey,
    read_arena::ReadArena,
    routed_action::{ActionRoute, RoutedOutput},
    state_arena::StateArena,
};

trait ErasedObserver {
    fn owner(&self) -> ComponentId;
    fn source(&self) -> ReadKey;
    fn last_revision(&self) -> u64;
    fn set_last_revision(&mut self, revision: u64);
    fn emit(&self, reads: &ReadArena, states: &StateArena) -> RuntimeResult<Option<RoutedOutput>>;
}

struct TypedObserver<T: 'static, Action: 'static, F> {
    owner: ComponentId,
    source: Read<T>,
    last_revision: u64,
    map: F,
    route: ActionRoute<Action>,
    marker: std::marker::PhantomData<fn() -> Action>,
}

impl<T, Action, F> ErasedObserver for TypedObserver<T, Action, F>
where
    T: 'static,
    Action: 'static,
    F: Fn(&T) -> Action + 'static,
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
    fn emit(&self, reads: &ReadArena, states: &StateArena) -> RuntimeResult<Option<RoutedOutput>> {
        Ok(self
            .route
            .route((self.map)(reads.get(self.source, states)?)))
    }
}

#[derive(Default)]
pub(crate) struct ObserverArena {
    observers: Vec<Box<dyn ErasedObserver>>,
}

impl ObserverArena {
    pub(crate) fn insert<T, Action, F>(
        &mut self,
        owner: ComponentId,
        read: Read<T>,
        map: F,
        route: ActionRoute<Action>,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        Action: 'static,
        F: Fn(&T) -> Action + 'static,
    {
        reads.validate_read(owner, read)?;
        self.observers.push(Box::new(TypedObserver {
            owner,
            source: read,
            last_revision: 0,
            map,
            route,
            marker: std::marker::PhantomData,
        }));
        Ok(())
    }

    pub(crate) fn collect(
        &mut self,
        reads: &mut ReadArena,
        states: &StateArena,
        emit_initial: bool,
    ) -> RuntimeResult<Vec<RoutedOutput>> {
        let mut actions = Vec::new();
        for observer in &mut self.observers {
            reads.evaluate(observer.source(), states)?;
            let revision = reads.revision(observer.source())?;
            if revision != observer.last_revision() {
                if (emit_initial || observer.last_revision() != 0)
                    && let Some(action) = observer.emit(reads, states)?
                {
                    actions.push(action);
                }
                observer.set_last_revision(revision);
            }
        }
        Ok(actions)
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) {
        self.observers.retain(|observer| observer.owner() != owner);
    }

    pub(crate) fn live(&self) -> usize {
        self.observers.len()
    }
}
