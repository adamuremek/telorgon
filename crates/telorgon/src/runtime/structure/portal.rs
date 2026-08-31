use crate::ui::{MountWriter, UiNodeId};

use crate::runtime::{
    Component, ComponentId, RuntimeError, RuntimeResult, component_arena::ComponentStores,
    read_arena::ReadArena, routed_action::ActionRoute, state_arena::StateArena,
    transaction::StateTransaction,
};

use super::{ErasedStructure, StructureArena, StructureRuntime};

pub(super) struct PortalStructure<C, F>
where
    C: Component,
{
    owner: ComponentId,
    host: UiNodeId,
    factory: F,
    route: Option<ActionRoute<C::Action>>,
    child: Option<ComponentId>,
    marker: std::marker::PhantomData<fn() -> C>,
}

impl<C, F> PortalStructure<C, F>
where
    C: Component,
{
    pub(super) fn new(
        owner: ComponentId,
        host: UiNodeId,
        factory: F,
        route: Option<ActionRoute<C::Action>>,
    ) -> Self {
        Self {
            owner,
            host,
            factory,
            route,
            child: None,
            marker: std::marker::PhantomData,
        }
    }
}

impl<C, F> ErasedStructure for PortalStructure<C, F>
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
        if self.child.is_some() {
            return Ok(());
        }
        let mut writer = MountWriter::under(runtime.ui, self.host)
            .ok_or_else(|| RuntimeError::new("portal visual host is stale"))?;
        runtime.diagnostics.structural_containers_visited += 1;
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
        Ok(())
    }
}
