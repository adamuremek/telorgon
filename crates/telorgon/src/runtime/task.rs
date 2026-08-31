use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::runtime::task_host::{TaskCancellation, TaskHost};
use crate::runtime::{ComponentId, RuntimeError, RuntimeResult, routed_action::RoutedAction};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TaskId {
    view: u64,
    index: u32,
    generation: u32,
}

#[derive(Clone)]
struct TaskWake {
    state: Arc<TaskWakeState>,
}

struct TaskWakeState {
    pending: AtomicBool,
    notify: Box<dyn Fn() + Send + Sync>,
}

impl TaskWake {
    fn new(notify: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            state: Arc::new(TaskWakeState {
                pending: AtomicBool::new(false),
                notify: Box::new(notify),
            }),
        }
    }

    fn signal(&self) {
        if !self.state.pending.swap(true, Ordering::AcqRel) {
            (self.state.notify)();
        }
    }

    fn begin_turn(&self) {
        self.state.pending.store(false, Ordering::Release);
    }
}

pub(crate) struct TaskControl {
    cancelled: AtomicBool,
    wake: Mutex<Option<TaskWake>>,
}

impl TaskControl {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            wake: Mutex::new(None),
        })
    }

    fn attach_wake(&self, wake: TaskWake) {
        *lock(&self.wake) = Some(wake);
    }

    fn cancel(&self) {
        if !self.cancelled.swap(true, Ordering::AcqRel)
            && let Some(wake) = lock(&self.wake).as_ref()
        {
            wake.signal();
        }
    }

    fn cancel_without_wake(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Cloneable cancellation authority for one component-scoped task.
#[derive(Clone)]
pub struct TaskHandle {
    control: Arc<TaskControl>,
}

impl TaskHandle {
    pub fn cancel(&self) {
        self.control.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.control.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum SenderRejection {
    Full,
    Closed,
}

/// Lossless result of a bounded progress send attempt.
#[derive(Debug, PartialEq, Eq)]
pub enum TaskSendError<Action> {
    Full(Action),
    Closed(Action),
}

struct SenderGate {
    state: Mutex<SenderGateState>,
}

struct SenderGateState {
    open: bool,
    queued: usize,
    capacity: usize,
}

impl SenderGate {
    fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(SenderGateState {
                open: true,
                queued: 0,
                capacity,
            }),
        })
    }

    fn close(&self) {
        lock(&self.state).open = false;
    }

    fn delivered(&self) {
        let mut state = lock(&self.state);
        state.queued = state.queued.saturating_sub(1);
    }
}

struct LocalCompletion {
    task: TaskId,
    target: ComponentId,
    type_id: TypeId,
    value: Box<dyn Any>,
    terminal: bool,
    gate: Arc<SenderGate>,
}

struct SendCompletion {
    task: TaskId,
    target: ComponentId,
    type_id: TypeId,
    value: Box<dyn Any + Send>,
    terminal: bool,
    gate: Arc<SenderGate>,
}

#[derive(Clone)]
pub(crate) struct LocalSenderCore {
    task: TaskId,
    target: ComponentId,
    type_id: TypeId,
    gate: Arc<SenderGate>,
    completions: Rc<RefCell<VecDeque<LocalCompletion>>>,
    wake: TaskWake,
}

impl LocalSenderCore {
    fn try_send(&self, value: Box<dyn Any>) -> Result<(), (SenderRejection, Box<dyn Any>)> {
        let mut state = lock(&self.gate.state);
        if !state.open {
            return Err((SenderRejection::Closed, value));
        }
        if state.queued >= state.capacity {
            return Err((SenderRejection::Full, value));
        }
        state.queued += 1;
        self.completions.borrow_mut().push_back(LocalCompletion {
            task: self.task,
            target: self.target,
            type_id: self.type_id,
            value,
            terminal: false,
            gate: self.gate.clone(),
        });
        drop(state);
        self.wake.signal();
        Ok(())
    }

    fn finish(&self, value: Box<dyn Any>) {
        let mut state = lock(&self.gate.state);
        if !state.open {
            return;
        }
        state.open = false;
        self.completions.borrow_mut().push_back(LocalCompletion {
            task: self.task,
            target: self.target,
            type_id: self.type_id,
            value,
            terminal: true,
            gate: self.gate.clone(),
        });
        drop(state);
        self.wake.signal();
    }
}

#[derive(Clone)]
pub(crate) struct SendSenderCore {
    task: TaskId,
    target: ComponentId,
    type_id: TypeId,
    gate: Arc<SenderGate>,
    completions: Arc<Mutex<VecDeque<SendCompletion>>>,
    wake: TaskWake,
}

impl SendSenderCore {
    fn try_send(
        &self,
        value: Box<dyn Any + Send>,
    ) -> Result<(), (SenderRejection, Box<dyn Any + Send>)> {
        let mut state = lock(&self.gate.state);
        if !state.open {
            return Err((SenderRejection::Closed, value));
        }
        if state.queued >= state.capacity {
            return Err((SenderRejection::Full, value));
        }
        state.queued += 1;
        lock(&self.completions).push_back(SendCompletion {
            task: self.task,
            target: self.target,
            type_id: self.type_id,
            value,
            terminal: false,
            gate: self.gate.clone(),
        });
        drop(state);
        self.wake.signal();
        Ok(())
    }

    fn finish(&self, value: Box<dyn Any + Send>) {
        let mut state = lock(&self.gate.state);
        if !state.open {
            return;
        }
        state.open = false;
        lock(&self.completions).push_back(SendCompletion {
            task: self.task,
            target: self.target,
            type_id: self.type_id,
            value,
            terminal: true,
            gate: self.gate.clone(),
        });
        drop(state);
        self.wake.signal();
    }
}

/// Bounded progress sender for a UI-thread local task.
pub struct LocalTaskSender<Action: 'static> {
    core: LocalSenderCore,
    marker: PhantomData<fn(Action)>,
}

impl<Action: 'static> Clone for LocalTaskSender<Action> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            marker: PhantomData,
        }
    }
}

impl<Action: 'static> LocalTaskSender<Action> {
    pub fn try_send(&self, action: Action) -> Result<(), TaskSendError<Action>> {
        match self.core.try_send(Box::new(action)) {
            Ok(()) => Ok(()),
            Err((SenderRejection::Full, value)) => Err(TaskSendError::Full(
                *value
                    .downcast()
                    .expect("local task action type is retained"),
            )),
            Err((SenderRejection::Closed, value)) => Err(TaskSendError::Closed(
                *value
                    .downcast()
                    .expect("local task action type is retained"),
            )),
        }
    }
}

/// Bounded, worker-safe progress sender whose messages become later-turn component actions.
pub struct TaskSender<Action: Send + 'static> {
    core: SendSenderCore,
    marker: PhantomData<fn(Action)>,
}

impl<Action: Send + 'static> Clone for TaskSender<Action> {
    fn clone(&self) -> Self {
        Self {
            core: self.core.clone(),
            marker: PhantomData,
        }
    }
}

impl<Action: Send + 'static> TaskSender<Action> {
    pub fn try_send(&self, action: Action) -> Result<(), TaskSendError<Action>> {
        match self.core.try_send(Box::new(action)) {
            Ok(()) => Ok(()),
            Err((SenderRejection::Full, value)) => Err(TaskSendError::Full(
                *value.downcast().expect("send task action type is retained"),
            )),
            Err((SenderRejection::Closed, value)) => Err(TaskSendError::Closed(
                *value.downcast().expect("send task action type is retained"),
            )),
        }
    }
}

type LocalTaskFuture = Pin<Box<dyn Future<Output = Box<dyn Any>> + 'static>>;
type SendTaskFuture = Pin<Box<dyn Future<Output = Box<dyn Any + Send>> + Send + 'static>>;
type LocalTaskBuild = Box<dyn FnOnce(LocalSenderCore) -> LocalTaskFuture>;
type SendTaskBuild = Box<dyn FnOnce(SendSenderCore) -> SendTaskFuture + Send>;

pub(crate) enum PendingTaskStart {
    Local {
        type_id: TypeId,
        control: Arc<TaskControl>,
        capacity: usize,
        build: LocalTaskBuild,
    },
    Send {
        type_id: TypeId,
        control: Arc<TaskControl>,
        capacity: usize,
        build: SendTaskBuild,
    },
}

impl PendingTaskStart {
    pub(crate) fn local<Action, F>(future: F) -> (Self, TaskHandle)
    where
        Action: 'static,
        F: Future<Output = Action> + 'static,
    {
        Self::local_with_sender(0, move |_| future)
    }

    pub(crate) fn local_with_sender<Action, F, Fut>(capacity: usize, build: F) -> (Self, TaskHandle)
    where
        Action: 'static,
        F: FnOnce(LocalTaskSender<Action>) -> Fut + 'static,
        Fut: Future<Output = Action> + 'static,
    {
        let control = TaskControl::new();
        let handle = TaskHandle {
            control: control.clone(),
        };
        let build = Box::new(move |core: LocalSenderCore| {
            let sender = LocalTaskSender {
                core,
                marker: PhantomData,
            };
            Box::pin(async move { Box::new(build(sender).await) as Box<dyn Any> })
                as LocalTaskFuture
        });
        (
            Self::Local {
                type_id: TypeId::of::<Action>(),
                control,
                capacity,
                build,
            },
            handle,
        )
    }

    pub(crate) fn send<Action, F>(future: F) -> (Self, TaskHandle)
    where
        Action: Send + 'static,
        F: Future<Output = Action> + Send + 'static,
    {
        Self::send_with_sender(0, move |_| future)
    }

    pub(crate) fn send_with_sender<Action, F, Fut>(capacity: usize, build: F) -> (Self, TaskHandle)
    where
        Action: Send + 'static,
        F: FnOnce(TaskSender<Action>) -> Fut + Send + 'static,
        Fut: Future<Output = Action> + Send + 'static,
    {
        let control = TaskControl::new();
        let handle = TaskHandle {
            control: control.clone(),
        };
        let build = Box::new(move |core: SendSenderCore| {
            let sender = TaskSender {
                core,
                marker: PhantomData,
            };
            Box::pin(async move { Box::new(build(sender).await) as Box<dyn Any + Send> })
                as SendTaskFuture
        });
        (
            Self::Send {
                type_id: TypeId::of::<Action>(),
                control,
                capacity,
                build,
            },
            handle,
        )
    }

    fn supported_by(&self, host: &dyn TaskHost) -> bool {
        match self {
            Self::Local { .. } => host.supports_local(),
            Self::Send { .. } => host.supports_send(),
        }
    }
}

struct TaskSlot {
    generation: u32,
    owner: Option<ComponentId>,
    control: Option<Arc<TaskControl>>,
    gate: Option<Arc<SenderGate>>,
    cancellation: Option<Box<dyn TaskCancellation>>,
}

pub(crate) struct TaskDrain {
    pub(crate) actions: VecDeque<RoutedAction>,
    pub(crate) delivered: usize,
    pub(crate) completed: usize,
    pub(crate) stale: usize,
    pub(crate) queue_depth: usize,
}

pub(crate) enum TaskStart {
    Started,
    Cancelled,
}

pub(crate) struct TaskArena {
    view: u64,
    slots: Vec<TaskSlot>,
    free: Vec<u32>,
    live: usize,
    local_completions: Rc<RefCell<VecDeque<LocalCompletion>>>,
    send_completions: Arc<Mutex<VecDeque<SendCompletion>>>,
    wake: TaskWake,
}

impl TaskArena {
    pub(crate) fn new(view: u64, wake: impl Fn() + Send + Sync + 'static) -> Self {
        Self {
            view,
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
            local_completions: Rc::new(RefCell::new(VecDeque::new())),
            send_completions: Arc::new(Mutex::new(VecDeque::new())),
            wake: TaskWake::new(wake),
        }
    }

    pub(crate) fn validate_starts(
        starts: &[PendingTaskStart],
        host: &dyn TaskHost,
    ) -> RuntimeResult<()> {
        if starts.iter().all(|start| start.supported_by(host)) {
            Ok(())
        } else {
            Err(RuntimeError::new(
                "component transaction requested a task unsupported by this host",
            ))
        }
    }

    pub(crate) fn start(
        &mut self,
        owner: ComponentId,
        start: PendingTaskStart,
        host: &mut dyn TaskHost,
    ) -> RuntimeResult<TaskStart> {
        let (control, gate, spawn) = match start {
            PendingTaskStart::Local {
                type_id,
                control,
                capacity,
                build,
            } => {
                let gate = SenderGate::new(capacity);
                let task = self.allocate(owner, control.clone(), gate.clone());
                control.attach_wake(self.wake.clone());
                if control.cancelled.load(Ordering::Acquire) {
                    self.release(task, true);
                    return Ok(TaskStart::Cancelled);
                }
                let core = LocalSenderCore {
                    task,
                    target: owner,
                    type_id,
                    gate: gate.clone(),
                    completions: self.local_completions.clone(),
                    wake: self.wake.clone(),
                };
                let terminal = core.clone();
                let future = build(core);
                let result = host.spawn_local(Box::pin(async move {
                    terminal.finish(future.await);
                }));
                (control, gate, (task, result))
            }
            PendingTaskStart::Send {
                type_id,
                control,
                capacity,
                build,
            } => {
                let gate = SenderGate::new(capacity);
                let task = self.allocate(owner, control.clone(), gate.clone());
                control.attach_wake(self.wake.clone());
                if control.cancelled.load(Ordering::Acquire) {
                    self.release(task, true);
                    return Ok(TaskStart::Cancelled);
                }
                let core = SendSenderCore {
                    task,
                    target: owner,
                    type_id,
                    gate: gate.clone(),
                    completions: self.send_completions.clone(),
                    wake: self.wake.clone(),
                };
                let terminal = core.clone();
                let future = build(core);
                let result = host.spawn_send(Box::pin(async move {
                    terminal.finish(future.await);
                }));
                (control, gate, (task, result))
            }
        };
        let (task, result) = spawn;
        match result {
            Ok(cancellation) => {
                let slot = self.slot_mut(task)?;
                slot.control = Some(control);
                slot.gate = Some(gate);
                slot.cancellation = Some(cancellation);
                Ok(TaskStart::Started)
            }
            Err(error) => {
                self.release(task, false);
                Err(error)
            }
        }
    }

    pub(crate) fn begin_turn(&self) {
        self.wake.begin_turn();
    }

    pub(crate) fn finish_turn(&self) {
        if self.is_ready() {
            self.wake.signal();
        }
    }

    pub(crate) fn cancel_requested(&mut self) -> usize {
        let tasks = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.owner.and_then(|_| {
                    slot.control
                        .as_ref()
                        .is_some_and(|control| control.cancelled.load(Ordering::Acquire))
                        .then_some(TaskId {
                            view: self.view,
                            index: index as u32,
                            generation: slot.generation,
                        })
                })
            })
            .collect::<Vec<_>>();
        let count = tasks.len();
        for task in tasks {
            self.release(task, true);
        }
        count
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) -> usize {
        let tasks = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                (slot.owner == Some(owner)).then_some(TaskId {
                    view: self.view,
                    index: index as u32,
                    generation: slot.generation,
                })
            })
            .collect::<Vec<_>>();
        let count = tasks.len();
        for task in tasks {
            self.release(task, true);
        }
        count
    }

    pub(crate) fn cancel_all(&mut self) -> usize {
        let tasks = self
            .slots
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| {
                slot.owner.map(|_| TaskId {
                    view: self.view,
                    index: index as u32,
                    generation: slot.generation,
                })
            })
            .collect::<Vec<_>>();
        let count = tasks.len();
        for task in tasks {
            self.release(task, true);
        }
        count
    }

    pub(crate) fn is_ready(&self) -> bool {
        if !self.local_completions.borrow().is_empty() || !lock(&self.send_completions).is_empty() {
            return true;
        }
        self.slots.iter().any(|slot| {
            slot.owner.is_some()
                && slot
                    .control
                    .as_ref()
                    .is_some_and(|control| control.cancelled.load(Ordering::Acquire))
        })
    }

    pub(crate) fn drain(&mut self, limit: usize) -> TaskDrain {
        let queue_depth =
            self.local_completions.borrow().len() + lock(&self.send_completions).len();
        let mut actions = VecDeque::new();
        let mut delivered = 0;
        let mut completed = 0;
        let mut stale = 0;

        while delivered + stale < limit {
            let completion = self.local_completions.borrow_mut().pop_front();
            let Some(completion) = completion else {
                break;
            };
            if !completion.terminal {
                completion.gate.delivered();
            }
            if self.accepts(completion.task, completion.target) {
                if completion.terminal {
                    self.release(completion.task, false);
                    completed += 1;
                }
                actions.push_back(RoutedAction {
                    target: completion.target,
                    type_id: completion.type_id,
                    value: completion.value,
                });
                delivered += 1;
            } else {
                stale += 1;
            }
        }

        while delivered + stale < limit {
            let completion = lock(&self.send_completions).pop_front();
            let Some(completion) = completion else {
                break;
            };
            if !completion.terminal {
                completion.gate.delivered();
            }
            if self.accepts(completion.task, completion.target) {
                if completion.terminal {
                    self.release(completion.task, false);
                    completed += 1;
                }
                let value: Box<dyn Any> = completion.value;
                actions.push_back(RoutedAction {
                    target: completion.target,
                    type_id: completion.type_id,
                    value,
                });
                delivered += 1;
            } else {
                stale += 1;
            }
        }

        TaskDrain {
            actions,
            delivered,
            completed,
            stale,
            queue_depth,
        }
    }

    pub(crate) fn live(&self) -> usize {
        self.live
    }

    fn allocate(
        &mut self,
        owner: ComponentId,
        control: Arc<TaskControl>,
        gate: Arc<SenderGate>,
    ) -> TaskId {
        let (index, generation) = if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.owner = Some(owner);
            slot.control = Some(control);
            slot.gate = Some(gate);
            (index, slot.generation)
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(TaskSlot {
                generation: 1,
                owner: Some(owner),
                control: Some(control),
                gate: Some(gate),
                cancellation: None,
            });
            (index, 1)
        };
        self.live += 1;
        TaskId {
            view: self.view,
            index,
            generation,
        }
    }

    fn accepts(&self, task: TaskId, owner: ComponentId) -> bool {
        self.slot(task)
            .is_some_and(|slot| slot.owner == Some(owner))
    }

    fn slot(&self, task: TaskId) -> Option<&TaskSlot> {
        (task.view == self.view)
            .then(|| self.slots.get(task.index as usize))
            .flatten()
            .filter(|slot| slot.generation == task.generation)
    }

    fn slot_mut(&mut self, task: TaskId) -> RuntimeResult<&mut TaskSlot> {
        if task.view != self.view {
            return Err(RuntimeError::new("task handle belongs to another view"));
        }
        self.slots
            .get_mut(task.index as usize)
            .filter(|slot| slot.generation == task.generation)
            .ok_or_else(|| RuntimeError::new("task handle is stale"))
    }

    fn release(&mut self, task: TaskId, cancel: bool) {
        let Some(slot) = self
            .slots
            .get_mut(task.index as usize)
            .filter(|slot| task.view == self.view && slot.generation == task.generation)
        else {
            return;
        };
        if slot.owner.is_none() {
            return;
        }
        if let Some(gate) = slot.gate.take() {
            gate.close();
        }
        if cancel {
            if let Some(control) = slot.control.as_ref() {
                control.cancel_without_wake();
            }
            if let Some(mut cancellation) = slot.cancellation.take() {
                cancellation.cancel();
            }
        }
        slot.control = None;
        slot.cancellation = None;
        slot.owner = None;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(task.index);
        self.live -= 1;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}
