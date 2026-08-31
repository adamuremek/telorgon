use std::any::TypeId;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::ui::{MountWriter, MountedUi, UiNodeId};

use crate::runtime::{
    Component, ComponentDiagnostics, ComponentId, Read, RuntimeError, RuntimeResult, State,
    binding::BindingArena,
    component_arena::{ComponentArena, ComponentStores},
    input_route::InputRouteArena,
    observer::ObserverArena,
    read_arena::ReadArena,
    routed_action::{ActionRoute, ActionRouteFactory},
    scheduler::TimerArena,
    state_arena::StateArena,
    task::TaskArena,
    transaction::StateTransaction,
};

mod portal;

use portal::PortalStructure;

pub(crate) struct StructureRuntime<'a> {
    pub(crate) components: &'a mut ComponentArena,
    pub(crate) ui: &'a mut MountedUi,
    pub(crate) states: &'a mut StateArena,
    pub(crate) reads: &'a mut ReadArena,
    pub(crate) bindings: &'a mut BindingArena,
    pub(crate) observers: &'a mut ObserverArena,
    pub(crate) input_routes: &'a mut InputRouteArena,
    pub(crate) tasks: &'a mut TaskArena,
    pub(crate) timers: &'a mut TimerArena,
    pub(crate) diagnostics: &'a mut ComponentDiagnostics,
}

trait ErasedSwitchFactory {
    fn component_type(&self) -> TypeId;
    fn mount(
        &self,
        owner: ComponentId,
        host: UiNodeId,
        runtime: &mut StructureRuntime<'_>,
        structures: &mut StructureArena,
    ) -> RuntimeResult<ComponentId>;
}

struct TypedSwitchFactory<C: Component, F> {
    factory: F,
    route: Option<ActionRouteFactory<C::Action>>,
    marker: std::marker::PhantomData<fn() -> C>,
}

impl<C, F> ErasedSwitchFactory for TypedSwitchFactory<C, F>
where
    C: Component,
    F: Fn() -> C + 'static,
{
    fn component_type(&self) -> TypeId {
        TypeId::of::<C>()
    }

    fn mount(
        &self,
        owner: ComponentId,
        host: UiNodeId,
        runtime: &mut StructureRuntime<'_>,
        structures: &mut StructureArena,
    ) -> RuntimeResult<ComponentId> {
        let mut writer = MountWriter::under(runtime.ui, host)
            .ok_or_else(|| RuntimeError::new("structural container host is stale"))?;
        let mut stores = ComponentStores {
            states: runtime.states,
            reads: runtime.reads,
            bindings: runtime.bindings,
            observers: runtime.observers,
            structures,
            input_routes: runtime.input_routes,
            tasks: runtime.tasks,
            timers: runtime.timers,
            diagnostics: runtime.diagnostics,
        };
        let child = runtime.components.mount_child(
            owner,
            (self.factory)(),
            &mut writer,
            self.route.as_ref().map(|route| route.create(owner)),
            &mut stores,
        )?;
        Ok(child)
    }
}

/// One explicit keyed branch for [`crate::runtime::Ui::switch`].
pub struct SwitchBranch<K> {
    key: K,
    factory: Box<dyn ErasedSwitchFactory>,
}

impl<K> SwitchBranch<K> {
    pub fn new<C, F>(key: K, factory: F) -> Self
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        Self {
            key,
            factory: Box::new(TypedSwitchFactory::<C, F> {
                factory,
                route: None,
                marker: std::marker::PhantomData,
            }),
        }
    }

    /// Creates a branch whose child actions map into the selected switch owner's action type.
    pub fn map<C, F, ParentAction, Map>(key: K, factory: F, map: Map) -> Self
    where
        C: Component,
        F: Fn() -> C + 'static,
        ParentAction: 'static,
        Map: Fn(C::Action) -> ParentAction + 'static,
    {
        Self {
            key,
            factory: Box::new(TypedSwitchFactory::<C, F> {
                factory,
                route: Some(ActionRouteFactory::map(map)),
                marker: std::marker::PhantomData,
            }),
        }
    }

    /// Creates a branch whose child actions are deliberately consumed.
    pub fn consume<C, F>(key: K, factory: F) -> Self
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        Self {
            key,
            factory: Box::new(TypedSwitchFactory::<C, F> {
                factory,
                route: Some(ActionRouteFactory::consume()),
                marker: std::marker::PhantomData,
            }),
        }
    }

    /// Creates a branch whose child actions map directly into runtime commands.
    pub fn command<C, F, Map>(key: K, factory: F, map: Map) -> Self
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> crate::runtime::Command + 'static,
    {
        Self {
            key,
            factory: Box::new(TypedSwitchFactory::<C, F> {
                factory,
                route: Some(ActionRouteFactory::command(map)),
                marker: std::marker::PhantomData,
            }),
        }
    }
}

struct SwitchStructure<K: 'static> {
    owner: ComponentId,
    source: Read<K>,
    host: UiNodeId,
    branches: Vec<SwitchBranch<K>>,
    active: Option<(K, TypeId, ComponentId)>,
    last_revision: u64,
}

impl<K> ErasedStructure for SwitchStructure<K>
where
    K: Clone + Eq + Hash + 'static,
{
    fn owner(&self) -> ComponentId {
        self.owner
    }

    fn validate(
        &self,
        transaction: &StateTransaction,
        reads: &ReadArena,
        states: &StateArena,
    ) -> RuntimeResult<()> {
        reads.validate_staged(self.source, states, transaction, |selected| {
            self.branch(selected).map(|_| ())
        })
    }

    fn reconcile(
        &mut self,
        runtime: &mut StructureRuntime<'_>,
        structures: &mut StructureArena,
    ) -> RuntimeResult<()> {
        runtime.reads.evaluate(self.source.key, runtime.states)?;
        let revision = runtime.reads.revision(self.source.key)?;
        if revision == self.last_revision {
            return Ok(());
        }
        runtime.diagnostics.structural_containers_visited += 1;
        let selected = runtime.reads.get(self.source, runtime.states)?.clone();
        let branch_index = self.branch_index(&selected)?;
        let branch_type = self.branches[branch_index].factory.component_type();
        if self
            .active
            .as_ref()
            .is_some_and(|(key, ty, _)| key == &selected && *ty == branch_type)
        {
            self.last_revision = revision;
            runtime.diagnostics.structural_reused += 1;
            return Ok(());
        }
        if !runtime.ui.nodes.contains(self.host) {
            return Err(RuntimeError::new("structural container host is stale"));
        }
        let replacing = self.active.is_some();
        if let Some((_, _, child)) = self.active.take() {
            let mut stores = ComponentStores {
                states: runtime.states,
                reads: runtime.reads,
                bindings: runtime.bindings,
                observers: runtime.observers,
                structures,
                input_routes: runtime.input_routes,
                tasks: runtime.tasks,
                timers: runtime.timers,
                diagnostics: runtime.diagnostics,
            };
            runtime.components.unmount(child, runtime.ui, &mut stores)?;
            runtime.diagnostics.structural_removed += 1;
        }
        let child = self.branches[branch_index]
            .factory
            .mount(self.owner, self.host, runtime, structures)?;
        self.active = Some((selected, branch_type, child));
        self.last_revision = revision;
        runtime.diagnostics.structural_inserted += 1;
        runtime.diagnostics.structural_replaced += u64::from(replacing);
        Ok(())
    }
}

impl<K> SwitchStructure<K>
where
    K: Eq,
{
    fn branch(&self, selected: &K) -> RuntimeResult<&SwitchBranch<K>> {
        self.branches
            .iter()
            .find(|branch| &branch.key == selected)
            .ok_or_else(|| RuntimeError::new("keyed switch has no branch for the selected key"))
    }

    fn branch_index(&self, selected: &K) -> RuntimeResult<usize> {
        self.branches
            .iter()
            .position(|branch| &branch.key == selected)
            .ok_or_else(|| RuntimeError::new("keyed switch has no branch for the selected key"))
    }
}

struct KeyedMounted<T: 'static> {
    child: ComponentId,
    item: State<T>,
}

struct KeyedStructure<T: 'static, K, C, Key, Factory>
where
    C: Component,
{
    owner: ComponentId,
    source: Read<Vec<T>>,
    host: UiNodeId,
    key: Key,
    factory: Factory,
    route: Option<ActionRoute<C::Action>>,
    entries: Vec<(K, KeyedMounted<T>)>,
    last_revision: u64,
    marker: std::marker::PhantomData<fn() -> C>,
}

impl<T, K, C, Key, Factory> ErasedStructure for KeyedStructure<T, K, C, Key, Factory>
where
    T: Clone + PartialEq + 'static,
    K: Clone + Eq + Hash + 'static,
    C: Component,
    Key: Fn(&T) -> K + 'static,
    Factory: Fn(Read<T>) -> C + 'static,
{
    fn owner(&self) -> ComponentId {
        self.owner
    }

    fn validate(
        &self,
        transaction: &StateTransaction,
        reads: &ReadArena,
        states: &StateArena,
    ) -> RuntimeResult<()> {
        reads.validate_staged(self.source, states, transaction, |items| {
            validate_unique_keys(items, &self.key)
        })
    }

    fn reconcile(
        &mut self,
        runtime: &mut StructureRuntime<'_>,
        structures: &mut StructureArena,
    ) -> RuntimeResult<()> {
        runtime.reads.evaluate(self.source.key, runtime.states)?;
        let revision = runtime.reads.revision(self.source.key)?;
        if revision == self.last_revision {
            return Ok(());
        }
        runtime.diagnostics.structural_containers_visited += 1;
        let items = runtime.reads.get(self.source, runtime.states)?.clone();
        let keyed = items
            .into_iter()
            .map(|item| ((self.key)(&item), item))
            .collect::<Vec<_>>();
        let mut unique = HashSet::with_capacity(keyed.len());
        if keyed.iter().any(|(key, _)| !unique.insert(key.clone())) {
            return Err(RuntimeError::new(
                "keyed component container contains a duplicate key",
            ));
        }
        if !runtime.ui.nodes.contains(self.host) {
            return Err(RuntimeError::new("structural container host is stale"));
        }

        let mut old = std::mem::take(&mut self.entries)
            .into_iter()
            .enumerate()
            .map(|(index, (key, entry))| (key, (index, entry)))
            .collect::<HashMap<_, _>>();
        let mut next = Vec::with_capacity(keyed.len());
        for (new_index, (key, item)) in keyed.into_iter().enumerate() {
            if let Some((old_index, mounted)) = old.remove(&key) {
                if runtime.states.get(self.owner, mounted.item)? != &item {
                    runtime
                        .states
                        .replace_any(mounted.item.key, Box::new(item))?;
                    runtime.reads.invalidate_states(&[mounted.item.key]);
                }
                runtime.diagnostics.structural_reused += 1;
                if old_index != new_index {
                    runtime.diagnostics.structural_moved += 1;
                }
                next.push((key, mounted));
            } else {
                let item_state = runtime.states.insert(self.owner, item);
                let item_read = runtime.reads.insert_source::<T>(self.owner, item_state.key);
                let item_state = item_state.with_read(item_read);
                let mut writer = MountWriter::under(runtime.ui, self.host)
                    .ok_or_else(|| RuntimeError::new("structural container host is stale"))?;
                let mut stores = ComponentStores {
                    states: runtime.states,
                    reads: runtime.reads,
                    bindings: runtime.bindings,
                    observers: runtime.observers,
                    structures,
                    input_routes: runtime.input_routes,
                    tasks: runtime.tasks,
                    timers: runtime.timers,
                    diagnostics: runtime.diagnostics,
                };
                let child = runtime.components.mount_child(
                    self.owner,
                    (self.factory)(item_read),
                    &mut writer,
                    self.route.clone(),
                    &mut stores,
                )?;
                next.push((
                    key,
                    KeyedMounted {
                        child,
                        item: item_state,
                    },
                ));
                runtime.diagnostics.structural_inserted += 1;
            }
        }
        for (_, (_, mounted)) in old {
            let mut stores = ComponentStores {
                states: runtime.states,
                reads: runtime.reads,
                bindings: runtime.bindings,
                observers: runtime.observers,
                structures,
                input_routes: runtime.input_routes,
                tasks: runtime.tasks,
                timers: runtime.timers,
                diagnostics: runtime.diagnostics,
            };
            runtime
                .components
                .unmount(mounted.child, runtime.ui, &mut stores)?;
            runtime.reads.remove(mounted.item.read().key)?;
            runtime.states.remove(mounted.item.key)?;
            runtime.diagnostics.structural_removed += 1;
        }

        let mut before = None;
        for (_, mounted) in next.iter().rev() {
            let root = runtime.components.root(mounted.child)?.0;
            let positioned =
                runtime.ui.nodes.core(root).is_some_and(|core| {
                    core.parent == Some(self.host) && core.next_sibling == before
                });
            if !positioned && !runtime.ui.nodes.reparent_before(root, self.host, before) {
                return Err(RuntimeError::new(
                    "keyed component root could not be reordered",
                ));
            }
            before = Some(root);
        }
        self.entries = next;
        self.last_revision = revision;
        Ok(())
    }
}

trait ErasedStructure {
    fn owner(&self) -> ComponentId;
    fn validate(
        &self,
        transaction: &StateTransaction,
        reads: &ReadArena,
        states: &StateArena,
    ) -> RuntimeResult<()>;
    fn reconcile(
        &mut self,
        runtime: &mut StructureRuntime<'_>,
        structures: &mut StructureArena,
    ) -> RuntimeResult<()>;
}

struct WhenStructure<C, F>
where
    C: Component,
{
    owner: ComponentId,
    source: Read<bool>,
    host: UiNodeId,
    factory: F,
    route: Option<ActionRoute<C::Action>>,
    child: Option<ComponentId>,
    last_revision: u64,
    marker: std::marker::PhantomData<fn() -> C>,
}

impl<C, F> ErasedStructure for WhenStructure<C, F>
where
    C: Component,
    F: Fn() -> C + 'static,
{
    fn owner(&self) -> ComponentId {
        self.owner
    }

    fn validate(
        &self,
        _transaction: &StateTransaction,
        _reads: &ReadArena,
        _states: &StateArena,
    ) -> RuntimeResult<()> {
        Ok(())
    }

    fn reconcile(
        &mut self,
        runtime: &mut StructureRuntime<'_>,
        structures: &mut StructureArena,
    ) -> RuntimeResult<()> {
        runtime.reads.evaluate(self.source.key, runtime.states)?;
        let revision = runtime.reads.revision(self.source.key)?;
        if revision == self.last_revision {
            return Ok(());
        }
        runtime.diagnostics.structural_containers_visited += 1;
        let visible = *runtime.reads.get(self.source, runtime.states)?;
        match (visible, self.child) {
            (true, None) => {
                let mut writer = MountWriter::under(runtime.ui, self.host)
                    .ok_or_else(|| RuntimeError::new("structural container host is stale"))?;
                let mut stores = ComponentStores {
                    states: runtime.states,
                    reads: runtime.reads,
                    bindings: runtime.bindings,
                    observers: runtime.observers,
                    structures,
                    input_routes: runtime.input_routes,
                    tasks: runtime.tasks,
                    timers: runtime.timers,
                    diagnostics: runtime.diagnostics,
                };
                let child = runtime.components.mount_child(
                    self.owner,
                    (self.factory)(),
                    &mut writer,
                    self.route.clone(),
                    &mut stores,
                )?;
                self.child = Some(child);
                runtime.diagnostics.structural_inserted += 1;
            }
            (false, Some(child)) => {
                let mut stores = ComponentStores {
                    states: runtime.states,
                    reads: runtime.reads,
                    bindings: runtime.bindings,
                    observers: runtime.observers,
                    structures,
                    input_routes: runtime.input_routes,
                    tasks: runtime.tasks,
                    timers: runtime.timers,
                    diagnostics: runtime.diagnostics,
                };
                runtime.components.unmount(child, runtime.ui, &mut stores)?;
                self.child = None;
                runtime.diagnostics.structural_removed += 1;
            }
            _ => {
                runtime.diagnostics.structural_reused += usize::from(visible) as u64;
            }
        }
        self.last_revision = revision;
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct StructureArena {
    entries: Vec<Option<Box<dyn ErasedStructure>>>,
}

impl StructureArena {
    pub(crate) fn insert_portal<C, F>(
        &mut self,
        owner: ComponentId,
        host: UiNodeId,
        factory: F,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.insert_portal_routed(owner, host, factory, None)
    }

    pub(crate) fn insert_portal_map<C, F, ParentAction, Map>(
        &mut self,
        owner: ComponentId,
        host: UiNodeId,
        factory: F,
        map: Map,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        ParentAction: 'static,
        Map: Fn(C::Action) -> ParentAction + 'static,
    {
        self.insert_portal_routed(owner, host, factory, Some(ActionRoute::map(owner, map)))
    }

    pub(crate) fn insert_portal_consume<C, F>(
        &mut self,
        owner: ComponentId,
        host: UiNodeId,
        factory: F,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.insert_portal_routed(owner, host, factory, Some(ActionRoute::consume()))
    }

    pub(crate) fn insert_portal_command<C, F, Map>(
        &mut self,
        owner: ComponentId,
        host: UiNodeId,
        factory: F,
        map: Map,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> crate::runtime::Command + 'static,
    {
        self.insert_portal_routed(owner, host, factory, Some(ActionRoute::command(map)))
    }

    fn insert_portal_routed<C, F>(
        &mut self,
        owner: ComponentId,
        host: UiNodeId,
        factory: F,
        route: Option<ActionRoute<C::Action>>,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.entries
            .push(Some(Box::new(PortalStructure::<C, F>::new(
                owner, host, factory, route,
            ))));
        Ok(())
    }

    pub(crate) fn insert_when<C, F>(
        &mut self,
        owner: ComponentId,
        source: Read<bool>,
        host: UiNodeId,
        factory: F,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        reads.validate_read(owner, source)?;
        self.entries.push(Some(Box::new(WhenStructure::<C, F> {
            owner,
            source,
            host,
            factory,
            route: None,
            child: None,
            last_revision: 0,
            marker: std::marker::PhantomData,
        })));
        Ok(())
    }

    pub(crate) fn insert_when_map<C, F, ParentAction, Map>(
        &mut self,
        owner: ComponentId,
        source: Read<bool>,
        host: UiNodeId,
        factory: F,
        map: Map,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        ParentAction: 'static,
        Map: Fn(C::Action) -> ParentAction + 'static,
    {
        reads.validate_read(owner, source)?;
        self.entries.push(Some(Box::new(WhenStructure::<C, F> {
            owner,
            source,
            host,
            factory,
            route: Some(ActionRoute::map(owner, map)),
            child: None,
            last_revision: 0,
            marker: std::marker::PhantomData,
        })));
        Ok(())
    }

    pub(crate) fn insert_when_consume<C, F>(
        &mut self,
        owner: ComponentId,
        source: Read<bool>,
        host: UiNodeId,
        factory: F,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        reads.validate_read(owner, source)?;
        self.entries.push(Some(Box::new(WhenStructure::<C, F> {
            owner,
            source,
            host,
            factory,
            route: Some(ActionRoute::consume()),
            child: None,
            last_revision: 0,
            marker: std::marker::PhantomData,
        })));
        Ok(())
    }

    pub(crate) fn insert_when_command<C, F, Map>(
        &mut self,
        owner: ComponentId,
        source: Read<bool>,
        host: UiNodeId,
        factory: F,
        map: Map,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> crate::runtime::Command + 'static,
    {
        reads.validate_read(owner, source)?;
        self.entries.push(Some(Box::new(WhenStructure::<C, F> {
            owner,
            source,
            host,
            factory,
            route: Some(ActionRoute::command(map)),
            child: None,
            last_revision: 0,
            marker: std::marker::PhantomData,
        })));
        Ok(())
    }

    pub(crate) fn insert_keyed<T, K, C, Key, Factory>(
        &mut self,
        owner: ComponentId,
        source: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
    {
        self.insert_keyed_routed(owner, source, host, key, factory, None, reads)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_keyed_map<T, K, C, Key, Factory, ParentAction, Map>(
        &mut self,
        owner: ComponentId,
        source: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        map: Map,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
        ParentAction: 'static,
        Map: Fn(C::Action) -> ParentAction + 'static,
    {
        self.insert_keyed_routed(
            owner,
            source,
            host,
            key,
            factory,
            Some(ActionRoute::map(owner, map)),
            reads,
        )
    }

    pub(crate) fn insert_keyed_consume<T, K, C, Key, Factory>(
        &mut self,
        owner: ComponentId,
        source: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
    {
        self.insert_keyed_routed(
            owner,
            source,
            host,
            key,
            factory,
            Some(ActionRoute::consume()),
            reads,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_keyed_command<T, K, C, Key, Factory, Map>(
        &mut self,
        owner: ComponentId,
        source: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        map: Map,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
        Map: Fn(C::Action) -> crate::runtime::Command + 'static,
    {
        self.insert_keyed_routed(
            owner,
            source,
            host,
            key,
            factory,
            Some(ActionRoute::command(map)),
            reads,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_keyed_routed<T, K, C, Key, Factory>(
        &mut self,
        owner: ComponentId,
        source: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        route: Option<ActionRoute<C::Action>>,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
    {
        reads.validate_read(owner, source)?;
        self.entries
            .push(Some(Box::new(KeyedStructure::<T, K, C, Key, Factory> {
                owner,
                source,
                host,
                key,
                factory,
                route,
                entries: Vec::new(),
                last_revision: 0,
                marker: std::marker::PhantomData,
            })));
        Ok(())
    }

    pub(crate) fn insert_switch<K>(
        &mut self,
        owner: ComponentId,
        source: Read<K>,
        host: UiNodeId,
        branches: Vec<SwitchBranch<K>>,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        K: Clone + Eq + Hash + 'static,
    {
        reads.validate_read(owner, source)?;
        if branches.is_empty() {
            return Err(RuntimeError::new(
                "keyed switch requires at least one branch",
            ));
        }
        let mut keys = HashSet::with_capacity(branches.len());
        if branches
            .iter()
            .any(|branch| !keys.insert(branch.key.clone()))
        {
            return Err(RuntimeError::new(
                "keyed switch contains a duplicate branch key",
            ));
        }
        self.entries.push(Some(Box::new(SwitchStructure {
            owner,
            source,
            host,
            branches,
            active: None,
            last_revision: 0,
        })));
        Ok(())
    }

    pub(crate) fn reconcile(&mut self, runtime: &mut StructureRuntime<'_>) -> RuntimeResult<()> {
        let mut index = 0;
        while index < self.entries.len() {
            let Some(mut entry) = self.entries[index].take() else {
                index += 1;
                continue;
            };
            let result = entry.reconcile(runtime, self);
            self.entries[index] = Some(entry);
            result?;
            index += 1;
        }
        runtime.diagnostics.live_structural_containers = self.live();
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        transaction: &StateTransaction,
        reads: &ReadArena,
        states: &StateArena,
    ) -> RuntimeResult<()> {
        for entry in self.entries.iter().filter_map(Option::as_deref) {
            entry.validate(transaction, reads, states)?;
        }
        Ok(())
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) {
        for entry in &mut self.entries {
            if entry.as_ref().is_some_and(|entry| entry.owner() == owner) {
                *entry = None;
            }
        }
    }

    pub(crate) fn live(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_some()).count()
    }
}

fn validate_unique_keys<T, K>(items: &[T], key: &impl Fn(&T) -> K) -> RuntimeResult<()>
where
    K: Eq + Hash,
{
    let mut unique = HashSet::with_capacity(items.len());
    if items.iter().any(|item| !unique.insert(key(item))) {
        Err(RuntimeError::new(
            "keyed component container contains a duplicate key",
        ))
    } else {
        Ok(())
    }
}
