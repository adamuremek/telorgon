use std::any::TypeId;
use std::cell::RefCell;
use std::marker::PhantomData;

use crate::compose::{Component, ComponentInstanceId, Signal, SignalDependency, SignalSnapshot};

/// Host family selected by the sealed root passed to an entry point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RuntimeTarget {
    #[default]
    Application,
    ShellWidget,
    Compositor,
}

struct EvaluationFrame {
    owner: ComponentInstanceId,
    component_type: TypeId,
    component_name: &'static str,
    target: RuntimeTarget,
    dependencies: Vec<SignalDependency>,
}

thread_local! {
    static EVALUATION_STACK: RefCell<Vec<EvaluationFrame>> = const { RefCell::new(Vec::new()) };
}

struct EvaluationGuard {
    owner: ComponentInstanceId,
    active: bool,
}

impl EvaluationGuard {
    fn enter<C: Component>(owner: ComponentInstanceId, target: RuntimeTarget) -> Self {
        EVALUATION_STACK.with_borrow_mut(|stack| {
            stack.push(EvaluationFrame {
                owner,
                component_type: TypeId::of::<C>(),
                component_name: std::any::type_name::<C>(),
                target,
                dependencies: Vec::new(),
            });
        });
        Self {
            owner,
            active: true,
        }
    }

    fn finish(mut self) -> Vec<SignalDependency> {
        let frame = pop_evaluation(self.owner);
        self.active = false;
        frame.dependencies
    }
}

impl Drop for EvaluationGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = pop_evaluation(self.owner);
        }
    }
}

fn pop_evaluation(owner: ComponentInstanceId) -> EvaluationFrame {
    EVALUATION_STACK.with_borrow_mut(|stack| {
        let frame = stack
            .pop()
            .expect("Telorgon component evaluation stack is empty");
        assert_eq!(
            frame.owner, owner,
            "Telorgon component evaluation scopes were exited out of order"
        );
        frame
    })
}

pub(crate) fn evaluate<C, R>(
    owner: ComponentInstanceId,
    target: RuntimeTarget,
    callback: impl FnOnce() -> R,
) -> (R, Vec<SignalDependency>)
where
    C: Component,
{
    let guard = EvaluationGuard::enter::<C>(owner, target);
    let result = callback();
    let dependencies = guard.finish();
    (result, dependencies)
}

fn with_component_frame<C, R>(callback: impl FnOnce(&mut EvaluationFrame) -> R) -> R
where
    C: Component,
{
    EVALUATION_STACK.with_borrow_mut(|stack| {
        let frame = stack.last_mut().unwrap_or_else(|| {
            panic!(
                "{} requested Telorgon runtime context outside Component::view",
                std::any::type_name::<C>()
            )
        });
        assert_eq!(
            frame.component_type,
            TypeId::of::<C>(),
            "{} requested the evaluation context owned by {}",
            std::any::type_name::<C>(),
            frame.component_name
        );
        callback(frame)
    })
}

pub(crate) fn watch<C, T>(signal: &Signal<T>) -> SignalSnapshot<T>
where
    C: Component,
    T: Send + Sync + 'static,
{
    let snapshot = signal.snapshot();
    let dependency = signal.dependency(snapshot.revision);
    with_component_frame::<C, _>(|frame| {
        if let Some(existing) = frame
            .dependencies
            .iter_mut()
            .find(|existing| existing.identity() == dependency.identity())
        {
            *existing = dependency;
        } else {
            frame.dependencies.push(dependency);
        }
    });
    snapshot
}

pub(crate) fn target<C: Component>() -> RuntimeTarget {
    with_component_frame::<C, _>(|frame| frame.target)
}

pub(crate) fn owner<C: Component>() -> ComponentInstanceId {
    with_component_frame::<C, _>(|frame| frame.owner)
}

macro_rules! lifecycle_context {
    ($name:ident) => {
        pub struct $name<C: Component> {
            owner: ComponentInstanceId,
            request_update: bool,
            marker: PhantomData<fn(C)>,
        }

        impl<C: Component> $name<C> {
            #[doc(hidden)]
            pub const fn new(owner: ComponentInstanceId) -> Self {
                Self {
                    owner,
                    request_update: false,
                    marker: PhantomData,
                }
            }

            pub const fn component(&self) -> ComponentInstanceId {
                self.owner
            }

            pub fn request_update(&mut self) {
                self.request_update = true;
            }

            #[doc(hidden)]
            pub const fn update_requested(&self) -> bool {
                self.request_update
            }
        }
    };
}

lifecycle_context!(MountContext);
lifecycle_context!(InputsChangedContext);
lifecycle_context!(UnmountContext);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compose::{ComponentFields, View, text};

    struct Fixture;

    impl ComponentFields for Fixture {
        type InputSnapshot = ();

        fn update_inputs(&mut self, _incoming: Self) -> bool {
            false
        }

        fn capture_inputs(&self) -> Self::InputSnapshot {}

        fn restore_inputs(&mut self, _snapshot: Self::InputSnapshot) -> bool {
            false
        }
    }

    impl Component for Fixture {
        fn view(&self) -> impl View {
            text("fixture")
        }
    }

    #[test]
    fn evaluation_exposes_component_properties_and_restores_the_scope() {
        let owner = ComponentInstanceId::new(4, 2);
        let fixture = Fixture;
        let (_, dependencies) = evaluate::<Fixture, _>(owner, RuntimeTarget::ShellWidget, || {
            assert_eq!(fixture.component_instance(), owner);
            assert_eq!(fixture.runtime_target(), RuntimeTarget::ShellWidget);
        });
        assert!(dependencies.is_empty());
        EVALUATION_STACK.with_borrow(|stack| assert!(stack.is_empty()));
    }

    #[test]
    fn evaluation_scope_is_restored_after_a_panic() {
        let result = std::panic::catch_unwind(|| {
            evaluate::<Fixture, ()>(
                ComponentInstanceId::new(9, 3),
                RuntimeTarget::Application,
                || panic!("fixture panic"),
            );
        });
        assert!(result.is_err());
        EVALUATION_STACK.with_borrow(|stack| assert!(stack.is_empty()));
    }

    #[test]
    fn contextual_methods_reject_calls_outside_view_evaluation() {
        let result = std::panic::catch_unwind(|| Fixture.runtime_target());
        assert!(result.is_err());
    }
}
