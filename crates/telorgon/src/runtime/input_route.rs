use std::collections::HashMap;

use crate::input::{Activation, ChangeSource, ValueChangePhase};
use crate::ui::{UiEvent, UiNodeId};

use crate::runtime::{
    ComponentId, Read, RuntimeError, RuntimeResult,
    read_arena::ReadArena,
    routed_action::{ActionRoute, RoutedOutput},
    state_arena::StateArena,
};

type ButtonRoute =
    Box<dyn Fn(Activation, &mut ReadArena, &StateArena) -> RuntimeResult<Option<RoutedOutput>>>;
type ListenerRouteFn = Box<dyn Fn(&UiEvent) -> Option<RoutedOutput>>;
type ValueRoute = Box<dyn Fn(f32, ValueChangePhase, ChangeSource) -> Option<RoutedOutput>>;

struct ListenerRoute {
    mask: u16,
    route: ListenerRouteFn,
}

struct NodeRoutes {
    owner: ComponentId,
    button: Option<ButtonRoute>,
    value: Option<ValueRoute>,
    listeners: Vec<ListenerRoute>,
}

/// Component-owned input routes keyed by generational foundation node IDs. Concrete action types
/// remain inside the registered closures and never enter mounted UI storage.
#[derive(Default)]
pub(crate) struct InputRouteArena {
    nodes: HashMap<UiNodeId, NodeRoutes>,
}

impl InputRouteArena {
    pub(crate) fn insert_button<Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        action_route: ActionRoute<Action>,
        action: F,
    ) -> RuntimeResult<()>
    where
        Action: 'static,
        F: Fn() -> Action + 'static,
    {
        let routes = self.routes_mut(owner, node)?;
        if routes.button.is_some() {
            return Err(RuntimeError::new(
                "foundation node already has a component button route",
            ));
        }
        routes.button = Some(Box::new(move |_, _, _| Ok(action_route.route(action()))));
        Ok(())
    }

    pub(crate) fn insert_activation<Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        action_route: ActionRoute<Action>,
        action: F,
    ) -> RuntimeResult<()>
    where
        Action: 'static,
        F: Fn(Activation) -> Action + 'static,
    {
        let routes = self.routes_mut(owner, node)?;
        if routes.button.is_some() {
            return Err(RuntimeError::new(
                "foundation node already has a component activation route",
            ));
        }
        routes.button = Some(Box::new(move |activation, _, _| {
            Ok(action_route.route(action(activation)))
        }));
        Ok(())
    }

    pub(crate) fn insert_activation_fallible<Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        action_route: ActionRoute<Action>,
        action: F,
    ) -> RuntimeResult<()>
    where
        Action: 'static,
        F: Fn(Activation) -> RuntimeResult<Action> + 'static,
    {
        let routes = self.routes_mut(owner, node)?;
        if routes.button.is_some() {
            return Err(RuntimeError::new(
                "foundation node already has a component activation route",
            ));
        }
        routes.button = Some(Box::new(move |activation, _, _| {
            Ok(action_route.route(action(activation)?))
        }));
        Ok(())
    }

    pub(crate) fn insert_activation_read<T, Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        read: Read<T>,
        action_route: ActionRoute<Action>,
        action: F,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        Action: 'static,
        F: Fn(&T, Activation) -> Action + 'static,
    {
        reads.validate_read(owner, read)?;
        let routes = self.routes_mut(owner, node)?;
        if routes.button.is_some() {
            return Err(RuntimeError::new(
                "foundation node already has a component activation route",
            ));
        }
        routes.button = Some(Box::new(move |activation, reads, states| {
            reads.evaluate(read.key, states)?;
            Ok(action_route.route(action(reads.get(read, states)?, activation)))
        }));
        Ok(())
    }

    pub(crate) fn insert_activation_read_fallible<T, Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        read: Read<T>,
        action_route: ActionRoute<Action>,
        action: F,
        reads: &ReadArena,
    ) -> RuntimeResult<()>
    where
        T: 'static,
        Action: 'static,
        F: Fn(&T, Activation) -> RuntimeResult<Action> + 'static,
    {
        reads.validate_read(owner, read)?;
        let routes = self.routes_mut(owner, node)?;
        if routes.button.is_some() {
            return Err(RuntimeError::new(
                "foundation node already has a component activation route",
            ));
        }
        routes.button = Some(Box::new(move |activation, reads, states| {
            reads.evaluate(read.key, states)?;
            let action = action(reads.get(read, states)?, activation)?;
            Ok(action_route.route(action))
        }));
        Ok(())
    }

    pub(crate) fn insert_listener<Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        mask: u16,
        action_route: ActionRoute<Action>,
        listener: F,
    ) -> RuntimeResult<()>
    where
        Action: 'static,
        F: Fn(&UiEvent) -> Action + 'static,
    {
        if mask == 0 {
            return Err(RuntimeError::new(
                "component input listener mask must not be empty",
            ));
        }
        self.routes_mut(owner, node)?.listeners.push(ListenerRoute {
            mask,
            route: Box::new(move |event| action_route.route(listener(event))),
        });
        Ok(())
    }

    pub(crate) fn insert_value<Action, F>(
        &mut self,
        owner: ComponentId,
        node: UiNodeId,
        action_route: ActionRoute<Action>,
        action: F,
    ) -> RuntimeResult<()>
    where
        Action: 'static,
        F: Fn(f32, ValueChangePhase, ChangeSource) -> Action + 'static,
    {
        let routes = self.routes_mut(owner, node)?;
        if routes.value.is_some() {
            return Err(RuntimeError::new(
                "foundation node already has a component value route",
            ));
        }
        routes.value = Some(Box::new(move |value, phase, source| {
            action_route.route(action(value, phase, source))
        }));
        Ok(())
    }

    pub(crate) fn activate(
        &self,
        node: UiNodeId,
        source: ChangeSource,
        reads: &mut ReadArena,
        states: &StateArena,
    ) -> (bool, RuntimeResult<Option<RoutedOutput>>) {
        let Some(route) = self
            .nodes
            .get(&node)
            .and_then(|routes| routes.button.as_ref())
        else {
            return (false, Ok(None));
        };
        (true, route(Activation { source }, reads, states))
    }

    pub(crate) fn dispatch(
        &self,
        event: &UiEvent,
        listener_mask: u16,
    ) -> (bool, Vec<RoutedOutput>) {
        let Some(routes) = self.nodes.get(&event.current_target) else {
            return (false, Vec::new());
        };
        let mut matched = false;
        let outputs = routes
            .listeners
            .iter()
            .filter(|listener| listener.mask & listener_mask != 0)
            .filter_map(|listener| {
                matched = true;
                (listener.route)(event)
            })
            .collect();
        (matched, outputs)
    }

    pub(crate) fn change_value(
        &self,
        node: UiNodeId,
        value: f32,
        phase: ValueChangePhase,
        source: ChangeSource,
    ) -> (bool, Option<RoutedOutput>) {
        let Some(route) = self
            .nodes
            .get(&node)
            .and_then(|routes| routes.value.as_ref())
        else {
            return (false, None);
        };
        (true, route(value.clamp(0.0, 1.0), phase, source))
    }

    pub(crate) fn remove_owner(&mut self, owner: ComponentId) {
        self.nodes.retain(|_, routes| routes.owner != owner);
    }

    pub(crate) fn live(&self) -> usize {
        self.nodes
            .values()
            .map(|routes| {
                usize::from(routes.button.is_some())
                    + usize::from(routes.value.is_some())
                    + routes.listeners.len()
            })
            .sum()
    }

    fn routes_mut(&mut self, owner: ComponentId, node: UiNodeId) -> RuntimeResult<&mut NodeRoutes> {
        let routes = self.nodes.entry(node).or_insert_with(|| NodeRoutes {
            owner,
            button: None,
            value: None,
            listeners: Vec::new(),
        });
        if routes.owner != owner {
            return Err(RuntimeError::new(
                "foundation input route node belongs to another component",
            ));
        }
        Ok(routes)
    }
}
