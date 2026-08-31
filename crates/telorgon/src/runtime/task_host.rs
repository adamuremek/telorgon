use std::future::Future;
use std::pin::Pin;

use crate::runtime::{RuntimeError, RuntimeResult};

/// A UI-thread future accepted by an injected task host.
pub type LocalTask = Pin<Box<dyn Future<Output = ()> + 'static>>;

/// A worker-safe future accepted by an injected task host.
pub type SendTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Runtime-owned cancellation handle returned by a task host.
pub trait TaskCancellation: 'static {
    fn cancel(&mut self);
}

/// Executor-neutral capability used by the component runtime.
///
/// Implementations schedule work but never call component code. The futures supplied here only
/// enqueue typed results for a later runtime turn.
pub trait TaskHost: 'static {
    fn supports_local(&self) -> bool {
        false
    }

    fn supports_send(&self) -> bool {
        false
    }

    fn spawn_local(&mut self, _task: LocalTask) -> RuntimeResult<Box<dyn TaskCancellation>> {
        Err(RuntimeError::new(
            "local tasks are unsupported by this host",
        ))
    }

    fn spawn_send(&mut self, _task: SendTask) -> RuntimeResult<Box<dyn TaskCancellation>> {
        Err(RuntimeError::new("send tasks are unsupported by this host"))
    }
}

/// Explicit no-task capability used when a view has no injected executor.
#[derive(Copy, Clone, Debug, Default)]
pub struct UnsupportedTaskHost;

impl TaskHost for UnsupportedTaskHost {}
