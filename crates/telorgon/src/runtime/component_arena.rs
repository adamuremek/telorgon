use std::any::{Any, TypeId};
use std::marker::PhantomData;

use crate::ui::{MountWriter, MountedUi, UiRoot};

use crate::runtime::{
    Component, ComponentDiagnostics, ComponentId, CreateContext, LifecycleState, RuntimeError,
    RuntimeResult, Ui, UnmountContext, UpdateContext,
    binding::BindingArena,
    input_route::InputRouteArena,
    observer::ObserverArena,
    read_arena::ReadArena,
    routed_action::ActionRoute,
    scheduler::{PendingTimerStart, TimerArena},
    state_arena::StateArena,
    structure::StructureArena,
    task::{PendingTaskStart, TaskArena},
    task_host::TaskHost,
    transaction::{StateTransaction, TransactionCommit},
};

trait ErasedComponent {
    fn action_type(&self) -> TypeId;
    fn action(
        &mut self,
        owner: ComponentId,
        action: Box<dyn Any>,
        states: &mut StateArena,
        task_host: &dyn TaskHost,
        validate: &mut dyn FnMut(&StateTransaction, &StateArena) -> RuntimeResult<()>,
    ) -> RuntimeResult<(
        TransactionCommit,
        Vec<PendingTaskStart>,
        Vec<PendingTimerStart>,
    )>;
    fn unmount(&mut self, owner: ComponentId);
}

struct ComponentEntry<C: Component> {
    component: C,
    state: C::State,
}

impl<C: Component> ErasedComponent for ComponentEntry<C> {
    fn action_type(&self) -> TypeId {
        TypeId::of::<C::Action>()
    }

    fn action(
        &mut self,
        owner: ComponentId,
        action: Box<dyn Any>,
        states: &mut StateArena,
        task_host: &dyn TaskHost,
        validate: &mut dyn FnMut(&StateTransaction, &StateArena) -> RuntimeResult<()>,
    ) -> RuntimeResult<(
        TransactionCommit,
        Vec<PendingTaskStart>,
        Vec<PendingTimerStart>,
    )> {
        let action = action
            .downcast::<C::Action>()
            .map_err(|_| RuntimeError::new("component action has the wrong concrete type"))?;
        let mut context = UpdateContext::<C> {
            states,
            transaction: StateTransaction::new(owner),
            task_starts: Vec::new(),
            timer_starts: Vec::new(),
            timer_error: None,
            marker: PhantomData,
        };
        self.component
            .action(&mut self.state, *action, &mut context);
        validate(&context.transaction, context.states)?;
        TaskArena::validate_starts(&context.task_starts, task_host)?;
        context.finish()
    }

    fn unmount(&mut self, owner: ComponentId) {
        self.component.unmount(
            &mut self.state,
            &mut UnmountContext {
                owner,
                marker: PhantomData,
            },
        );
    }
}

struct ComponentSlot {
    generation: u32,
    lifecycle: LifecycleState,
    entry: Option<Box<dyn ErasedComponent>>,
    root: Option<UiRoot>,
    parent: Option<ComponentId>,
    children: Vec<ComponentId>,
}

pub(crate) struct ComponentArena {
    view: u64,
    slots: Vec<ComponentSlot>,
    free: Vec<u32>,
    live: usize,
}

pub(crate) struct ComponentStores<'a> {
    pub(crate) states: &'a mut StateArena,
    pub(crate) reads: &'a mut ReadArena,
    pub(crate) bindings: &'a mut BindingArena,
    pub(crate) observers: &'a mut ObserverArena,
    pub(crate) structures: &'a mut StructureArena,
    pub(crate) input_routes: &'a mut InputRouteArena,
    pub(crate) tasks: &'a mut TaskArena,
    pub(crate) timers: &'a mut TimerArena,
    pub(crate) diagnostics: &'a mut ComponentDiagnostics,
}

impl ComponentArena {
    pub(crate) fn new(view: u64) -> Self {
        Self {
            view,
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    pub(crate) fn mount_root<C: Component>(
        &mut self,
        component: C,
        writer: &mut MountWriter<'_, C::Action>,
        stores: &mut ComponentStores<'_>,
    ) -> (ComponentId, UiRoot) {
        let id = self.allocate();
        self.slot_mut(id).expect("new component slot").lifecycle = LifecycleState::Creating;
        let state = component.create(&mut CreateContext {
            owner: id,
            states: stores.states,
            reads: stores.reads,
        });
        stores.diagnostics.components_created += 1;

        self.slot_mut(id).expect("new component slot").lifecycle = LifecycleState::Mounting;
        let root = component.mount(
            &state,
            &mut Ui {
                owner: id,
                writer,
                states: stores.states,
                reads: stores.reads,
                bindings: stores.bindings,
                observers: stores.observers,
                structures: stores.structures,
                input_routes: stores.input_routes,
                action_route: ActionRoute::component(id),
            },
        );
        let slot = self.slot_mut(id).expect("new component slot");
        slot.entry = Some(Box::new(ComponentEntry { component, state }));
        slot.root = Some(root);
        slot.parent = None;
        slot.lifecycle = LifecycleState::Mounted;
        stores.diagnostics.components_mounted += 1;
        stores.diagnostics.live_components = self.live;
        stores.diagnostics.live_states = stores.states.live();
        stores.diagnostics.live_bindings = stores.bindings.live();
        (id, root)
    }

    pub(crate) fn mount_child<C: Component>(
        &mut self,
        parent: ComponentId,
        component: C,
        writer: &mut MountWriter<'_, C::Action>,
        route: Option<ActionRoute<C::Action>>,
        stores: &mut ComponentStores<'_>,
    ) -> RuntimeResult<ComponentId> {
        if self.slot(parent)?.lifecycle != LifecycleState::Mounted {
            return Err(RuntimeError::new(
                "child component parent is not accepting mounts",
            ));
        }
        let id = self.allocate();
        self.slot_mut(id)?.lifecycle = LifecycleState::Creating;
        let state = component.create(&mut CreateContext {
            owner: id,
            states: stores.states,
            reads: stores.reads,
        });
        stores.diagnostics.components_created += 1;

        self.slot_mut(id)?.lifecycle = LifecycleState::Mounting;
        let root = component.mount(
            &state,
            &mut Ui {
                owner: id,
                writer,
                states: stores.states,
                reads: stores.reads,
                bindings: stores.bindings,
                observers: stores.observers,
                structures: stores.structures,
                input_routes: stores.input_routes,
                action_route: route.unwrap_or_else(|| ActionRoute::component(id)),
            },
        );
        let slot = self.slot_mut(id)?;
        slot.entry = Some(Box::new(ComponentEntry { component, state }));
        slot.root = Some(root);
        slot.parent = Some(parent);
        slot.lifecycle = LifecycleState::Mounted;
        self.slot_mut(parent)?.children.push(id);
        stores.diagnostics.components_mounted += 1;
        stores.diagnostics.live_components = self.live;
        stores.diagnostics.live_states = stores.states.live();
        stores.diagnostics.live_bindings = stores.bindings.live();
        Ok(id)
    }

    pub(crate) fn action_erased(
        &mut self,
        target: ComponentId,
        action_type: TypeId,
        action: Box<dyn Any>,
        states: &mut StateArena,
        task_host: &dyn TaskHost,
        mut validate: impl FnMut(&StateTransaction, &StateArena) -> RuntimeResult<()>,
    ) -> RuntimeResult<(
        TransactionCommit,
        Vec<PendingTaskStart>,
        Vec<PendingTimerStart>,
    )> {
        let slot = self.slot_mut(target)?;
        if slot.lifecycle != LifecycleState::Mounted {
            return Err(RuntimeError::new("component is not accepting actions"));
        }
        let mut entry = slot
            .entry
            .take()
            .ok_or_else(|| RuntimeError::new("mounted component has no retained entry"))?;
        if entry.action_type() != action_type {
            slot.entry = Some(entry);
            return Err(RuntimeError::new(
                "component action route has the wrong type",
            ));
        }
        let result = entry.action(target, action, states, task_host, &mut validate);
        self.slot_mut(target)?.entry = Some(entry);
        result
    }

    pub(crate) fn unmount(
        &mut self,
        target: ComponentId,
        ui: &mut MountedUi,
        stores: &mut ComponentStores<'_>,
    ) -> RuntimeResult<()> {
        let root = self.unmount_logical(target, stores)?;
        if let Some(root) = root {
            ui.remove(root.0);
        }
        Ok(())
    }

    pub(crate) fn unmount_for_drop(
        &mut self,
        target: ComponentId,
        stores: &mut ComponentStores<'_>,
    ) -> RuntimeResult<()> {
        self.unmount_logical(target, stores).map(|_| ())
    }

    fn unmount_logical(
        &mut self,
        target: ComponentId,
        stores: &mut ComponentStores<'_>,
    ) -> RuntimeResult<Option<UiRoot>> {
        {
            let slot = self.slot_mut(target)?;
            if slot.lifecycle != LifecycleState::Mounted {
                return Err(RuntimeError::new("component is not mounted"));
            }
            slot.lifecycle = LifecycleState::Unmounting;
        }
        let cancelled = stores.tasks.remove_owner(target);
        stores.diagnostics.tasks_cancelled += cancelled as u64;
        stores.diagnostics.live_tasks = stores.tasks.live();
        let cancelled = stores.timers.remove_owner(target);
        stores.diagnostics.timers_cancelled += cancelled as u64;
        stores.diagnostics.live_timers = stores.timers.live();
        let (mut entry, root) = {
            let slot = self.slot_mut(target)?;
            (slot.entry.take(), slot.root.take())
        };
        let children = self.slot(target)?.children.clone();
        for child in children.into_iter().rev() {
            self.unmount_logical(child, stores)?;
        }
        if let Some(entry) = entry.as_mut() {
            entry.unmount(target);
        }
        stores.structures.remove_owner(target);
        stores.input_routes.remove_owner(target);
        stores.bindings.remove_owner(target);
        stores.observers.remove_owner(target);
        stores.reads.remove_owner(target);
        stores.states.remove_owner(target);
        let parent = self.slot(target)?.parent;
        if let Some(parent) = parent
            && let Ok(slot) = self.slot_mut(parent)
        {
            slot.children.retain(|child| *child != target);
        }
        let slot = self.slot_mut(target)?;
        slot.lifecycle = LifecycleState::Dead;
        slot.entry = None;
        slot.parent = None;
        slot.children.clear();
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(target.index);
        self.live -= 1;
        stores.diagnostics.components_unmounted += 1;
        stores.diagnostics.live_components = self.live;
        stores.diagnostics.live_states = stores.states.live();
        stores.diagnostics.live_bindings = stores.bindings.live();
        Ok(root)
    }

    pub(crate) fn lifecycle(&self, id: ComponentId) -> Option<LifecycleState> {
        self.slot(id).ok().map(|slot| slot.lifecycle)
    }

    pub(crate) fn root(&self, id: ComponentId) -> RuntimeResult<UiRoot> {
        self.slot(id)?
            .root
            .ok_or_else(|| RuntimeError::new("component has no mounted root"))
    }

    fn allocate(&mut self) -> ComponentId {
        let (index, generation) = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.lifecycle = LifecycleState::Allocated;
            (index, slot.generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(ComponentSlot {
                generation: 1,
                lifecycle: LifecycleState::Allocated,
                entry: None,
                root: None,
                parent: None,
                children: Vec::new(),
            });
            (index, 1)
        };
        self.live += 1;
        ComponentId {
            view: self.view,
            index,
            generation,
        }
    }

    fn slot(&self, id: ComponentId) -> RuntimeResult<&ComponentSlot> {
        if id.view != self.view {
            return Err(RuntimeError::new(
                "component handle belongs to another view",
            ));
        }
        self.slots
            .get(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .ok_or_else(|| RuntimeError::new("component handle is stale"))
    }

    fn slot_mut(&mut self, id: ComponentId) -> RuntimeResult<&mut ComponentSlot> {
        if id.view != self.view {
            return Err(RuntimeError::new(
                "component handle belongs to another view",
            ));
        }
        self.slots
            .get_mut(id.index as usize)
            .filter(|slot| slot.generation == id.generation)
            .ok_or_else(|| RuntimeError::new("component handle is stale"))
    }
}
