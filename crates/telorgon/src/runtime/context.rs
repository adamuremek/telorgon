use std::collections::VecDeque;
use std::future::Future;
use std::hash::Hash;
use std::marker::PhantomData;
use std::time::Duration;

use crate::input::{Activation, ChangeSource, ValueChangePhase};
use crate::ui::{
    BoxStyle, ControlHandle, MountWriter, MountedUi, Property, PropertyValue, UiEvent, UiNodeId,
};

use crate::runtime::{
    Component, ComponentId, MonotonicInstant, Read, RuntimeError, RuntimeResult, State,
    TimerHandle,
    binding::BindingArena,
    input_route::InputRouteArena,
    observer::ObserverArena,
    read_arena::ReadArena,
    routed_action::ActionRoute,
    scheduler::PendingTimerStart,
    state_arena::StateArena,
    structure::{StructureArena, SwitchBranch},
    task::{LocalTaskSender, PendingTaskStart, TaskHandle, TaskSender},
    transaction::StateTransaction,
};

/// Runtime-owned requests emitted by component action routes.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    RequestFrame,
}

#[doc(hidden)]
pub struct DriverContext<'a> {
    pub(crate) ui: &'a mut MountedUi,
    pub(crate) commands: &'a mut VecDeque<Command>,
    pub(crate) frame_requested: &'a mut bool,
}

impl DriverContext<'_> {
    pub(crate) fn command(&mut self, command: Command) {
        if command == Command::RequestFrame {
            *self.frame_requested = true;
        }
        self.commands.push_back(command);
    }
}

/// State-allocation context available only while a component is being created.
pub struct CreateContext<'a> {
    pub(crate) owner: ComponentId,
    pub(crate) states: &'a mut StateArena,
    pub(crate) reads: &'a mut ReadArena,
}

impl CreateContext<'_> {
    pub fn state<T: 'static>(&mut self, value: T) -> State<T> {
        let state = self.states.insert(self.owner, value);
        let read = self.reads.insert_source::<T>(self.owner, state.key);
        state.with_read(read)
    }

    pub fn map<T, U, F>(&mut self, source: Read<T>, map: F) -> RuntimeResult<Read<U>>
    where
        T: 'static,
        U: PartialEq + 'static,
        F: Fn(&T) -> U + 'static,
    {
        self.reads.map(self.owner, source, map)
    }

    pub fn zip<A, B, U, F>(
        &mut self,
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
        self.reads.zip(self.owner, left, right, map)
    }

    pub fn select<T>(
        &mut self,
        condition: Read<bool>,
        when_true: Read<T>,
        when_false: Read<T>,
    ) -> RuntimeResult<Read<T>>
    where
        T: Clone + PartialEq + 'static,
    {
        self.reads
            .select(self.owner, condition, when_true, when_false)
    }

    pub fn component(&self) -> ComponentId {
        self.owner
    }
}

/// Mount-only component facade over the current lower-level foundation writer.
pub struct Ui<'a, 'storage, Action: 'static> {
    pub(crate) owner: ComponentId,
    pub(crate) writer: &'a mut MountWriter<'storage, Action>,
    pub(crate) states: &'a StateArena,
    pub(crate) reads: &'a mut ReadArena,
    pub(crate) bindings: &'a mut BindingArena,
    pub(crate) observers: &'a mut ObserverArena,
    pub(crate) structures: &'a mut StructureArena,
    pub(crate) input_routes: &'a mut InputRouteArena,
    pub(crate) action_route: ActionRoute<Action>,
}

impl<'a, 'storage, Action: 'static> Ui<'a, 'storage, Action> {
    /// Accesses the temporary foundation writer. Component code cannot retain it past mount.
    pub fn foundation(&mut self) -> &mut MountWriter<'storage, Action> {
        self.writer
    }

    /// Clones the current controlled value during mount after evaluating its dependency chain.
    pub fn read<T>(&mut self, read: Read<T>) -> RuntimeResult<T>
    where
        T: Clone + 'static,
    {
        self.reads.validate_read(self.owner, read)?;
        self.reads.evaluate(read.key, self.states)?;
        Ok(self.reads.get(read, self.states)?.clone())
    }

    /// Creates a foundation button whose repeatable action factory remains owned by this
    /// component generation. The produced action does not need to implement `Clone`.
    pub fn button<F>(
        &mut self,
        host: UiNodeId,
        action: F,
        style: BoxStyle,
        content: impl FnOnce(&mut MountWriter<'storage, Action>),
    ) -> RuntimeResult<ControlHandle>
    where
        F: Fn() -> Action + 'static,
    {
        let control = self
            .writer
            .button_node_under(host, style, content)
            .ok_or_else(|| crate::runtime::RuntimeError::new("foundation button host is stale"))?;
        self.input_routes.insert_button(
            self.owner,
            control.node,
            self.action_route.clone(),
            action,
        )?;
        Ok(control)
    }

    /// Registers a completed, source-preserving activation route on a component-owned
    /// foundation node. Arm/cancel validation remains with the input/default-behavior owner.
    pub fn route_activation<F>(&mut self, node: UiNodeId, action: F) -> RuntimeResult<()>
    where
        F: Fn(Activation) -> Action + 'static,
    {
        self.input_routes
            .insert_activation(self.owner, node, self.action_route.clone(), action)
    }

    /// Registers a completed activation route whose typed action derivation may reject.
    pub fn route_activation_fallible<F>(&mut self, node: UiNodeId, action: F) -> RuntimeResult<()>
    where
        F: Fn(Activation) -> RuntimeResult<Action> + 'static,
    {
        self.input_routes.insert_activation_fallible(
            self.owner,
            node,
            self.action_route.clone(),
            action,
        )
    }

    /// Registers a completed activation route whose action is derived from the latest controlled
    /// read without mutating that value.
    pub fn route_activation_read<T, F>(
        &mut self,
        node: UiNodeId,
        read: Read<T>,
        action: F,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        F: Fn(&T, Activation) -> Action + 'static,
    {
        self.input_routes.insert_activation_read(
            self.owner,
            node,
            read,
            self.action_route.clone(),
            action,
            self.reads,
        )
    }

    /// Registers a read-aware activation route whose derivation may reject the current controlled
    /// value without fabricating an action.
    pub fn route_activation_read_fallible<T, F>(
        &mut self,
        node: UiNodeId,
        read: Read<T>,
        action: F,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        F: Fn(&T, Activation) -> RuntimeResult<Action> + 'static,
    {
        self.input_routes.insert_activation_read_fallible(
            self.owner,
            node,
            read,
            self.action_route.clone(),
            action,
            self.reads,
        )
    }

    /// Registers a normalized continuous-value route for a mounted value control.
    pub fn route_value<F>(&mut self, node: UiNodeId, action: F) -> RuntimeResult<()>
    where
        F: Fn(f32, ValueChangePhase, ChangeSource) -> Action + 'static,
    {
        self.input_routes
            .insert_value(self.owner, node, self.action_route.clone(), action)
    }

    /// Registers a neutral foundation listener that maps each matching event to a fresh owned
    /// component action.
    pub fn listen<F>(&mut self, node: UiNodeId, mask: u16, listener: F) -> RuntimeResult<()>
    where
        F: Fn(&UiEvent) -> Action + 'static,
    {
        self.writer.listen(node, mask);
        self.input_routes.insert_listener(
            self.owner,
            node,
            mask,
            self.action_route.clone(),
            listener,
        )
    }

    /// Binds one source state directly to one mounted property.
    pub fn bind<T>(&mut self, state: State<T>, property: Property<T>) -> RuntimeResult<()>
    where
        T: Clone + Into<PropertyValue> + 'static,
    {
        self.bind_read(state.read(), property)
    }

    pub fn bind_read<T>(&mut self, read: Read<T>, property: Property<T>) -> RuntimeResult<()>
    where
        T: Clone + Into<PropertyValue> + 'static,
    {
        self.bindings.insert(self.owner, read, property, self.reads)
    }

    /// Binds a controlled read to a mounted property through a retained projection.
    pub fn bind_map<T, U, F>(
        &mut self,
        read: Read<T>,
        property: Property<U>,
        map: F,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        U: Into<PropertyValue> + 'static,
        F: Fn(&T) -> U + 'static,
    {
        self.bindings
            .insert_map(self.owner, read, property, map, self.reads)
    }

    pub fn observe<T, F>(&mut self, read: Read<T>, map: F) -> RuntimeResult<()>
    where
        T: 'static,
        F: Fn(&T) -> Action + 'static,
    {
        self.observers
            .insert(self.owner, read, map, self.action_route.clone(), self.reads)
    }

    /// Mounts a logically owned child beneath an explicit visual layer host. Moving the host in
    /// the mounted node tree does not replace the child component or its state.
    pub fn portal<C, F>(&mut self, host: UiNodeId, factory: F) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.structures.insert_portal(self.owner, host, factory)
    }

    /// Mounts a portal child and maps its actions into this component's action type.
    pub fn portal_map<C, F, Map>(
        &mut self,
        host: UiNodeId,
        factory: F,
        map: Map,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> Action + 'static,
    {
        self.structures
            .insert_portal_map(self.owner, host, factory, map)
    }

    /// Mounts a portal child while deliberately consuming its actions.
    pub fn portal_consume<C, F>(&mut self, host: UiNodeId, factory: F) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.structures
            .insert_portal_consume(self.owner, host, factory)
    }

    /// Mounts a portal child and maps its actions directly into runtime commands.
    pub fn portal_command<C, F, Map>(
        &mut self,
        host: UiNodeId,
        factory: F,
        map: Map,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> Command + 'static,
    {
        self.structures
            .insert_portal_command(self.owner, host, factory, map)
    }

    /// Mounts one child component while `condition` is true. False unmounts the child and a later
    /// true value creates a fresh component generation.
    pub fn when<C, F>(
        &mut self,
        condition: Read<bool>,
        host: crate::ui::UiNodeId,
        factory: F,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.structures
            .insert_when(self.owner, condition, host, factory, self.reads)
    }

    /// Mounts a child with its own action type and explicitly maps emitted child actions into this
    /// component's action type.
    pub fn when_map<C, F, Map>(
        &mut self,
        condition: Read<bool>,
        host: crate::ui::UiNodeId,
        factory: F,
        map: Map,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> Action + 'static,
    {
        self.structures
            .insert_when_map(self.owner, condition, host, factory, map, self.reads)
    }

    /// Mounts a child while deliberately consuming actions it emits through runtime observers.
    pub fn when_consume<C, F>(
        &mut self,
        condition: Read<bool>,
        host: crate::ui::UiNodeId,
        factory: F,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
    {
        self.structures
            .insert_when_consume(self.owner, condition, host, factory, self.reads)
    }

    /// Mounts a child and maps its emitted observer actions directly into runtime commands.
    pub fn when_command<C, F, Map>(
        &mut self,
        condition: Read<bool>,
        host: crate::ui::UiNodeId,
        factory: F,
        map: Map,
    ) -> RuntimeResult<()>
    where
        C: Component,
        F: Fn() -> C + 'static,
        Map: Fn(C::Action) -> Command + 'static,
    {
        self.structures
            .insert_when_command(self.owner, condition, host, factory, map, self.reads)
    }

    /// Reconciles a homogeneous child collection by explicit local keys. Retained keys keep their
    /// component and mounted-node identities; changed item values update the child input read.
    pub fn for_each_keyed<T, K, C, Key, Factory>(
        &mut self,
        items: Read<Vec<T>>,
        host: crate::ui::UiNodeId,
        key: Key,
        factory: Factory,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
    {
        self.structures
            .insert_keyed(self.owner, items, host, key, factory, self.reads)
    }

    /// Reconciles keyed children while mapping each child's observer and foundation-input actions
    /// into this component's action type.
    pub fn for_each_keyed_map<T, K, C, Key, Factory, Map>(
        &mut self,
        items: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        map: Map,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
        Map: Fn(C::Action) -> Action + 'static,
    {
        self.structures
            .insert_keyed_map(self.owner, items, host, key, factory, map, self.reads)
    }

    /// Reconciles keyed children while deliberately consuming their observer and foundation-input
    /// actions.
    pub fn for_each_keyed_consume<T, K, C, Key, Factory>(
        &mut self,
        items: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
    {
        self.structures
            .insert_keyed_consume(self.owner, items, host, key, factory, self.reads)
    }

    /// Reconciles keyed children while mapping their observer and foundation-input actions to
    /// runtime commands.
    pub fn for_each_keyed_command<T, K, C, Key, Factory, Map>(
        &mut self,
        items: Read<Vec<T>>,
        host: UiNodeId,
        key: Key,
        factory: Factory,
        map: Map,
    ) -> RuntimeResult<()>
    where
        T: Clone + PartialEq + 'static,
        K: Clone + Eq + Hash + 'static,
        C: Component,
        Key: Fn(&T) -> K + 'static,
        Factory: Fn(Read<T>) -> C + 'static,
        Map: Fn(C::Action) -> Command + 'static,
    {
        self.structures
            .insert_keyed_command(self.owner, items, host, key, factory, map, self.reads)
    }

    /// Retains exactly one explicitly keyed branch and replaces its component generation when the
    /// selected key changes.
    pub fn switch<K>(
        &mut self,
        selected: Read<K>,
        host: crate::ui::UiNodeId,
        branches: Vec<SwitchBranch<K>>,
    ) -> RuntimeResult<()>
    where
        K: Clone + Eq + Hash + 'static,
    {
        self.structures
            .insert_switch(self.owner, selected, host, branches, self.reads)
    }

    pub fn component(&self) -> ComponentId {
        self.owner
    }
}

/// Owner-authorized component update transaction.
pub struct UpdateContext<'a, C: Component> {
    pub(crate) states: &'a mut StateArena,
    pub(crate) transaction: StateTransaction,
    pub(crate) task_starts: Vec<PendingTaskStart>,
    pub(crate) timer_starts: Vec<PendingTimerStart>,
    pub(crate) timer_error: Option<RuntimeError>,
    pub(crate) marker: PhantomData<fn(C)>,
}

impl<C: Component> UpdateContext<'_, C> {
    pub fn get<T: Clone + 'static>(&self, state: State<T>) -> RuntimeResult<T> {
        self.transaction.get(self.states, state)
    }

    pub fn set<T: PartialEq + 'static>(&mut self, state: State<T>, value: T) -> RuntimeResult<()> {
        self.transaction.set(self.states, state, value)
    }

    pub fn replace_always<T: 'static>(&mut self, state: State<T>, value: T) -> RuntimeResult<()> {
        self.transaction.replace_always(self.states, state, value)
    }

    /// Stages a one-shot typed action for an absolute host-clock deadline.
    pub fn timer_at(&mut self, deadline: MonotonicInstant, action: C::Action) -> TimerHandle {
        let (start, handle) = PendingTimerStart::once(deadline, action);
        self.timer_starts.push(start);
        handle
    }

    /// Stages a one-shot typed action relative to an explicitly supplied host-clock instant.
    pub fn timer_after(
        &mut self,
        now: MonotonicInstant,
        delay: Duration,
        action: C::Action,
    ) -> RuntimeResult<TimerHandle> {
        let Some(deadline) = now.checked_add(delay) else {
            let error =
                RuntimeError::new("component timer deadline overflowed the monotonic clock");
            self.timer_error = Some(error.clone());
            return Err(error);
        };
        Ok(self.timer_at(deadline, action))
    }

    /// Stages a repeating typed-action factory at an absolute first deadline.
    ///
    /// Missed periods coalesce: a delayed host turn emits at most one action for this timer and
    /// advances its next deadline beyond the supplied current time.
    pub fn interval_at<F>(
        &mut self,
        first_deadline: MonotonicInstant,
        period: Duration,
        action: F,
    ) -> RuntimeResult<TimerHandle>
    where
        F: FnMut() -> C::Action + 'static,
    {
        match PendingTimerStart::repeating(first_deadline, period, action) {
            Ok((start, handle)) => {
                self.timer_starts.push(start);
                Ok(handle)
            }
            Err(error) => {
                self.timer_error = Some(error.clone());
                Err(error)
            }
        }
    }

    /// Stages UI-thread work whose result becomes a typed action in a later runtime turn.
    pub fn spawn<F>(&mut self, future: F) -> TaskHandle
    where
        F: Future<Output = C::Action> + 'static,
    {
        self.spawn_local(future)
    }

    /// Stages a non-`Send` future for the injected UI task host.
    pub fn spawn_local<F>(&mut self, future: F) -> TaskHandle
    where
        F: Future<Output = C::Action> + 'static,
    {
        let (start, handle) = PendingTaskStart::local(future);
        self.task_starts.push(start);
        handle
    }

    /// Stages worker-safe work for the injected send-task host.
    pub fn spawn_send<F>(&mut self, future: F) -> TaskHandle
    where
        C::Action: Send,
        F: Future<Output = C::Action> + Send + 'static,
    {
        let (start, handle) = PendingTaskStart::send(future);
        self.task_starts.push(start);
        handle
    }

    /// Stages local work with a bounded progress sender.
    pub fn spawn_with_sender<F, Fut>(
        &mut self,
        capacity: usize,
        build: F,
    ) -> RuntimeResult<TaskHandle>
    where
        F: FnOnce(LocalTaskSender<C::Action>) -> Fut + 'static,
        Fut: Future<Output = C::Action> + 'static,
    {
        self.spawn_local_with_sender(capacity, build)
    }

    /// Stages local work with bounded, later-turn progress actions.
    pub fn spawn_local_with_sender<F, Fut>(
        &mut self,
        capacity: usize,
        build: F,
    ) -> RuntimeResult<TaskHandle>
    where
        F: FnOnce(LocalTaskSender<C::Action>) -> Fut + 'static,
        Fut: Future<Output = C::Action> + 'static,
    {
        if capacity == 0 {
            return Err(crate::runtime::RuntimeError::new(
                "local task sender capacity must be greater than zero",
            ));
        }
        let (start, handle) = PendingTaskStart::local_with_sender(capacity, build);
        self.task_starts.push(start);
        Ok(handle)
    }

    /// Stages worker-safe work with a bounded, thread-safe progress sender.
    pub fn spawn_send_with_sender<F, Fut>(
        &mut self,
        capacity: usize,
        build: F,
    ) -> RuntimeResult<TaskHandle>
    where
        C::Action: Send,
        F: FnOnce(TaskSender<C::Action>) -> Fut + Send + 'static,
        Fut: Future<Output = C::Action> + Send + 'static,
    {
        if capacity == 0 {
            return Err(crate::runtime::RuntimeError::new(
                "send task sender capacity must be greater than zero",
            ));
        }
        let (start, handle) = PendingTaskStart::send_with_sender(capacity, build);
        self.task_starts.push(start);
        Ok(handle)
    }

    /// Nested helpers share this transaction and commit only after the outer action returns.
    pub fn transaction<R>(&mut self, update: impl FnOnce(&mut Self) -> R) -> R {
        update(self)
    }

    pub(crate) fn finish(
        self,
    ) -> RuntimeResult<(
        crate::runtime::transaction::TransactionCommit,
        Vec<PendingTaskStart>,
        Vec<PendingTimerStart>,
    )> {
        if let Some(error) = self.timer_error {
            return Err(error);
        }
        let commit = self.transaction.commit(self.states)?;
        Ok((commit, self.task_starts, self.timer_starts))
    }
}

/// Cleanup-only context passed after a component has stopped accepting work.
pub struct UnmountContext<'a> {
    pub(crate) owner: ComponentId,
    pub(crate) marker: PhantomData<&'a mut ()>,
}

impl UnmountContext<'_> {
    pub fn component(&self) -> ComponentId {
        self.owner
    }
}
