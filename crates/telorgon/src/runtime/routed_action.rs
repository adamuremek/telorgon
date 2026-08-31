use std::any::{Any, TypeId};
use std::rc::Rc;

use crate::runtime::{Command, ComponentId};

pub(crate) struct RoutedAction {
    pub(crate) target: ComponentId,
    pub(crate) type_id: TypeId,
    pub(crate) value: Box<dyn Any>,
}

pub(crate) enum RoutedOutput {
    Action(RoutedAction),
    Command(Command),
}

pub(crate) struct ActionRoute<Action: 'static> {
    route: Rc<dyn Fn(Action) -> Option<RoutedOutput>>,
}

pub(crate) struct ActionRouteFactory<Action: 'static> {
    create: Rc<dyn Fn(ComponentId) -> ActionRoute<Action>>,
}

impl<Action: 'static> ActionRoute<Action> {
    pub(crate) fn component(target: ComponentId) -> Self {
        Self {
            route: Rc::new(move |action| {
                Some(RoutedOutput::Action(RoutedAction {
                    target,
                    type_id: TypeId::of::<Action>(),
                    value: Box::new(action),
                }))
            }),
        }
    }

    pub(crate) fn map<ParentAction, F>(target: ComponentId, map: F) -> Self
    where
        ParentAction: 'static,
        F: Fn(Action) -> ParentAction + 'static,
    {
        Self {
            route: Rc::new(move |action| {
                Some(RoutedOutput::Action(RoutedAction {
                    target,
                    type_id: TypeId::of::<ParentAction>(),
                    value: Box::new(map(action)),
                }))
            }),
        }
    }

    pub(crate) fn command<F>(map: F) -> Self
    where
        F: Fn(Action) -> Command + 'static,
    {
        Self {
            route: Rc::new(move |action| Some(RoutedOutput::Command(map(action)))),
        }
    }

    pub(crate) fn consume() -> Self {
        Self {
            route: Rc::new(|_| None),
        }
    }

    pub(crate) fn route(&self, action: Action) -> Option<RoutedOutput> {
        (self.route)(action)
    }
}

impl<Action: 'static> Clone for ActionRoute<Action> {
    fn clone(&self) -> Self {
        Self {
            route: self.route.clone(),
        }
    }
}

impl<Action: 'static> ActionRouteFactory<Action> {
    pub(crate) fn map<ParentAction, F>(map: F) -> Self
    where
        ParentAction: 'static,
        F: Fn(Action) -> ParentAction + 'static,
    {
        let map = Rc::new(map);
        Self {
            create: Rc::new(move |target| {
                let map = map.clone();
                ActionRoute::map(target, move |action| map(action))
            }),
        }
    }

    pub(crate) fn command<F>(map: F) -> Self
    where
        F: Fn(Action) -> Command + 'static,
    {
        let map = Rc::new(map);
        Self {
            create: Rc::new(move |_| {
                let map = map.clone();
                ActionRoute::command(move |action| map(action))
            }),
        }
    }

    pub(crate) fn consume() -> Self {
        Self {
            create: Rc::new(|_| ActionRoute::consume()),
        }
    }

    pub(crate) fn create(&self, target: ComponentId) -> ActionRoute<Action> {
        (self.create)(target)
    }
}
