//! Baseline accessible command toolbar over shared command and composite owners.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;

use crate::core::{ColorRgba8, EdgeInsets};
use crate::input::{
    ChangeSource, CompositeChange, CompositeEdgeBehavior, CompositeError, CompositeItem,
    CompositeNavigationCommand, CompositeNavigationPolicy, CompositeOrientation,
    CompositeSelectionBehavior, CompositeStateMachine, DisabledItemPolicy, WritingDirection,
};
use crate::runtime::{ComponentId, RuntimeError, RuntimeResult, Ui};
use crate::ui::{
    Background, BoxStyle, ControlHandle, CornerRadii, Flow, LayoutStyle, Property, SemanticActions,
    SemanticCheckState, SemanticName, SemanticNode, SemanticRelationship, SemanticRelationshipKind,
    SemanticRole, SemanticState, SizeRule, SizeRule2D, UiNodeId,
};

use crate::application_components::{
    CheckState, CommandSpec, DensityClass, DensityMetrics, ResolvedCommandState,
};

/// Logical orientation accepted by the baseline one-dimensional toolbar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolbarOrientation {
    #[default]
    Horizontal,
    Vertical,
}

impl ToolbarOrientation {
    const fn composite(self) -> CompositeOrientation {
        match self {
            Self::Horizontal => CompositeOrientation::Horizontal,
            Self::Vertical => CompositeOrientation::Vertical,
        }
    }

    const fn flow(self) -> Flow {
        match self {
            Self::Horizontal => Flow::Horizontal,
            Self::Vertical => Flow::Vertical,
        }
    }
}

/// Fixed navigation and disabled-item discovery policy for one toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarNavigationPolicy {
    pub orientation: ToolbarOrientation,
    pub edge_behavior: CompositeEdgeBehavior,
    pub disabled_items: DisabledItemPolicy,
}

impl ToolbarNavigationPolicy {
    const fn composite(self) -> CompositeNavigationPolicy {
        CompositeNavigationPolicy {
            orientation: self.orientation.composite(),
            edge_behavior: self.edge_behavior,
            disabled_items: self.disabled_items,
            selection: CompositeSelectionBehavior::Independent,
        }
    }
}

impl Default for ToolbarNavigationPolicy {
    fn default() -> Self {
        Self {
            orientation: ToolbarOrientation::Horizontal,
            edge_behavior: CompositeEdgeBehavior::Stop,
            disabled_items: DisabledItemPolicy::Include,
        }
    }
}

/// Navigation result from the shared composite owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolbarTransition<K> {
    pub change: CompositeChange<K>,
}

/// A request to invoke the active enabled command. It commits no application state itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToolbarCommandRequest<K> {
    pub command: K,
    pub source: ChangeSource,
}

/// Toolbar-specific wrapper over the single neutral active-descendant owner.
#[derive(Clone, Debug)]
pub struct ToolbarBehavior<K> {
    composite: CompositeStateMachine<K>,
}

impl<K> ToolbarBehavior<K>
where
    K: Copy + Eq + Hash,
{
    pub fn new(
        items: impl IntoIterator<Item = CompositeItem<K>>,
        policy: ToolbarNavigationPolicy,
    ) -> Result<Self, CompositeError<K>> {
        let mut composite = CompositeStateMachine::new(policy.composite());
        composite.update_items(items)?;
        composite.enter(None)?;
        Ok(Self { composite })
    }

    pub fn active_command(&self) -> Option<K> {
        self.composite.active_descendant()
    }

    pub fn update_items(
        &mut self,
        items: impl IntoIterator<Item = CompositeItem<K>>,
    ) -> Result<ToolbarTransition<K>, CompositeError<K>> {
        Ok(ToolbarTransition {
            change: self.composite.update_items(items)?,
        })
    }

    pub fn navigate(
        &mut self,
        command: CompositeNavigationCommand,
        direction: WritingDirection,
    ) -> Result<ToolbarTransition<K>, CompositeError<K>> {
        Ok(ToolbarTransition {
            change: self.composite.navigate(command, direction)?,
        })
    }

    pub fn request_active_command(
        &mut self,
        source: ChangeSource,
    ) -> Result<ToolbarCommandRequest<K>, CompositeError<K>> {
        let request = self.composite.request_active_selection(source)?;
        Ok(ToolbarCommandRequest {
            command: request.key,
            source: request.source,
        })
    }
}

/// Typed toolbar visual slots. Layout orientation remains behavior policy, not theme policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolbarStyle {
    pub container: BoxStyle,
    pub item: BoxStyle,
    pub checked_item: BoxStyle,
    pub label_color: ColorRgba8,
    pub disabled_label_color: ColorRgba8,
    pub label_size: f32,
    pub gap: f32,
}

impl Default for ToolbarStyle {
    fn default() -> Self {
        let item = BoxStyle {
            padding: EdgeInsets::all(6.0),
            corner_radii: CornerRadii::all(4.0),
            ..BoxStyle::default()
        };
        Self {
            container: BoxStyle {
                padding: EdgeInsets::all(2.0),
                ..BoxStyle::default()
            },
            item,
            checked_item: BoxStyle {
                background: Background::Color(ColorRgba8::rgba(66, 91, 139, 110)),
                ..item
            },
            label_color: ColorRgba8::rgba(235, 238, 244, 255),
            disabled_label_color: ColorRgba8::rgba(145, 151, 164, 255),
            label_size: 14.0,
            gap: 4.0,
        }
    }
}

/// One accepted command action created after toolbar activation validation.
#[derive(Debug, PartialEq, Eq)]
pub struct ToolbarInvocation<K, A> {
    command: K,
    action: A,
    source: ChangeSource,
    checked: Option<CheckState>,
}

impl<K, A> ToolbarInvocation<K, A> {
    pub fn command(&self) -> &K {
        &self.command
    }

    pub const fn source(&self) -> ChangeSource {
        self.source
    }

    pub const fn checked(&self) -> Option<CheckState> {
        self.checked
    }

    pub fn into_action(self) -> A {
        self.action
    }
}

/// Immutable reusable command toolbar configuration.
pub struct Toolbar<K: 'static, A: 'static> {
    label: String,
    commands: Vec<CommandSpec<K, A>>,
    navigation: ToolbarNavigationPolicy,
    density: DensityMetrics,
    style: ToolbarStyle,
}

impl<K, A> Toolbar<K, A>
where
    K: Copy + Eq + Hash + 'static,
    A: 'static,
{
    pub fn new(
        label: impl Into<String>,
        commands: impl IntoIterator<Item = CommandSpec<K, A>>,
    ) -> Result<Self, ToolbarError<K>> {
        let label = label.into();
        if label.trim().is_empty() {
            return Err(ToolbarError::MissingAccessibleName);
        }
        let commands: Vec<_> = commands.into_iter().collect();
        let Some(first) = commands.first() else {
            return Err(ToolbarError::Empty);
        };
        let owner = first.owner();
        let mut keys = HashSet::with_capacity(commands.len());
        for command in &commands {
            if !keys.insert(*command.id()) {
                return Err(ToolbarError::DuplicateCommand(*command.id()));
            }
            if command.owner() != owner {
                return Err(ToolbarError::OwnerMismatch {
                    expected: owner,
                    actual: command.owner(),
                });
            }
        }
        Ok(Self {
            label,
            commands,
            navigation: ToolbarNavigationPolicy::default(),
            density: DensityMetrics::baseline(DensityClass::Standard),
            style: ToolbarStyle::default(),
        })
    }

    pub fn navigation(mut self, navigation: ToolbarNavigationPolicy) -> Self {
        self.navigation = navigation;
        self
    }

    pub fn density(mut self, density: DensityMetrics) -> Self {
        self.density = density;
        self
    }

    pub fn style(mut self, style: ToolbarStyle) -> Self {
        self.style = style;
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn commands(&self) -> &[CommandSpec<K, A>] {
        &self.commands
    }

    fn behavior(
        &self,
        states: impl IntoIterator<Item = ResolvedCommandState>,
    ) -> Result<ToolbarBehavior<K>, CompositeError<K>> {
        let items = self
            .commands
            .iter()
            .zip(states)
            .map(|(command, state)| CompositeItem {
                key: *command.id(),
                enabled: state.enabled(),
            });
        ToolbarBehavior::new(items, self.navigation)
    }

    pub fn mount<HostAction, Map>(
        &self,
        ui: &mut Ui<'_, '_, HostAction>,
        host: UiNodeId,
        map: Map,
    ) -> RuntimeResult<ToolbarRef<K, A>>
    where
        HostAction: 'static,
        Map: Fn(ToolbarInvocation<K, A>) -> HostAction + 'static,
    {
        let mut resolved = Vec::with_capacity(self.commands.len());
        for command in &self.commands {
            resolved.push(ResolvedToolbarCommand {
                command: command.clone(),
                state: command.resolve_state(ui)?,
            });
        }
        let behavior = Rc::new(RefCell::new(
            self.behavior(resolved.iter().map(|command| command.state))
                .map_err(|_| RuntimeError::new("invalid toolbar composite state"))?,
        ));
        let active = behavior.borrow().active_command();
        let minimum = self.density.effective_minimum();
        let display_commands: Vec<_> = resolved
            .iter()
            .map(|command| {
                (
                    *command.command.id(),
                    command.command.label().to_owned(),
                    command.state,
                )
            })
            .collect();
        let mut mounted = Vec::with_capacity(display_commands.len());
        let toolbar = ui
            .foundation()
            .button_node_under(host, self.style.container, |writer| {
                writer.container(
                    BoxStyle::default(),
                    LayoutStyle {
                        flow: self.navigation.orientation.flow(),
                        gap: self.style.gap,
                        ..LayoutStyle::default()
                    },
                    |writer| {
                        for (key, label, state) in display_commands {
                            let checked = state.checked() != Some(CheckState::Unchecked)
                                && state.checked().is_some();
                            let mut item_style = if checked {
                                self.style.checked_item
                            } else {
                                self.style.item
                            };
                            item_style.min_size = SizeRule2D {
                                width: SizeRule::Px(minimum.width()),
                                height: SizeRule::Px(minimum.height()),
                            };
                            let color = if state.enabled() {
                                self.style.label_color
                            } else {
                                self.style.disabled_label_color
                            };
                            let control = writer.action_node(item_style, false, |writer| {
                                writer.text(&label, color, self.style.label_size);
                            });
                            mounted.push((key, label, state, control));
                        }
                    },
                );
            })
            .ok_or_else(|| RuntimeError::new("application toolbar host is stale"))?;

        let commands = Rc::new(resolved);
        let map: Rc<dyn Fn(ToolbarInvocation<K, A>) -> HostAction> = Rc::new(map);
        let mut item_refs = Vec::with_capacity(mounted.len());
        for (key, label, state, control) in &mounted {
            let name = ui.foundation().intern(label);
            let semantic_checked = state.checked().map(check_state_semantic);
            ui.foundation()
                .semantic_node(
                    control.node,
                    SemanticNode {
                        role: SemanticRole::Button,
                        name: SemanticName::Text(name),
                        state: SemanticState {
                            disabled: !state.enabled(),
                            checked: semantic_checked,
                            ..SemanticState::default()
                        },
                        actions: if state.enabled() {
                            SemanticActions::ACTIVATE
                        } else {
                            SemanticActions::NONE
                        },
                        ..SemanticNode::default()
                    },
                )
                .map_err(|error| {
                    RuntimeError::new(format!("invalid toolbar item semantics: {error:?}"))
                })?;
            if !state.enabled() {
                ui.foundation().disabled(control.node, true);
            }
            if semantic_checked.is_some_and(|checked| checked != SemanticCheckState::Unchecked) {
                ui.foundation().checked(control.node, true);
            }
            if state.enabled() {
                let key = *key;
                let route_commands = commands.clone();
                let route_map = map.clone();
                ui.route_activation_fallible(control.node, move |activation| {
                    let invocation = invoke_command(&route_commands, key, activation.source)
                        .map_err(|_| RuntimeError::new("toolbar item invocation failed"))?;
                    Ok(route_map(invocation))
                })?;
            }
            item_refs.push(ToolbarItemRef {
                command: *key,
                control: *control,
                enabled: state.enabled(),
                checked: state.checked(),
            });
        }

        let toolbar_name = ui.foundation().intern(&self.label);
        let mut relationships: Vec<_> = mounted
            .iter()
            .map(|(_, _, _, control)| SemanticRelationship {
                kind: SemanticRelationshipKind::Owns,
                target: control.node,
            })
            .collect();
        let active_enabled = active.and_then(|key| {
            mounted.iter().find_map(|(item_key, _, state, control)| {
                (*item_key == key).then_some((control.node, state.enabled()))
            })
        });
        if let Some((active_node, _)) = active_enabled {
            relationships.push(SemanticRelationship {
                kind: SemanticRelationshipKind::ActiveDescendant,
                target: active_node,
            });
        }
        ui.foundation()
            .semantic_node(
                toolbar.node,
                SemanticNode {
                    role: SemanticRole::Toolbar,
                    name: SemanticName::Text(toolbar_name),
                    state: SemanticState {
                        focusable: true,
                        ..SemanticState::default()
                    },
                    actions: if active_enabled.is_some_and(|(_, enabled)| enabled) {
                        SemanticActions::FOCUS | SemanticActions::ACTIVATE
                    } else {
                        SemanticActions::FOCUS
                    },
                    relationships,
                    ..SemanticNode::default()
                },
            )
            .map_err(|error| RuntimeError::new(format!("invalid toolbar semantics: {error:?}")))?;

        if commands.iter().any(|command| command.state.enabled()) {
            let route_behavior = behavior.clone();
            let route_commands = commands.clone();
            let route_map = map.clone();
            ui.route_activation_fallible(toolbar.node, move |activation| {
                let request = route_behavior
                    .borrow_mut()
                    .request_active_command(activation.source)
                    .map_err(|_| RuntimeError::new("active toolbar command is unavailable"))?;
                let invocation = invoke_command(&route_commands, request.command, request.source)
                    .map_err(|_| {
                    RuntimeError::new("active toolbar command invocation failed")
                })?;
                Ok(route_map(invocation))
            })?;
        }

        Ok(ToolbarRef {
            toolbar,
            items: item_refs,
            behavior,
            commands,
        })
    }
}

impl<K, A> Clone for Toolbar<K, A>
where
    K: Clone + 'static,
    A: 'static,
{
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            commands: self.commands.clone(),
            navigation: self.navigation,
            density: self.density,
            style: self.style,
        }
    }
}

impl<K, A> fmt::Debug for Toolbar<K, A>
where
    K: fmt::Debug + 'static,
    A: 'static,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Toolbar")
            .field("label", &self.label)
            .field("commands", &self.commands)
            .field("navigation", &self.navigation)
            .field("density", &self.density)
            .field("style", &self.style)
            .finish()
    }
}

struct ResolvedToolbarCommand<K: 'static, A: 'static> {
    command: CommandSpec<K, A>,
    state: ResolvedCommandState,
}

/// Mounted toolbar handle retaining the same neutral composite and command owners.
pub struct ToolbarRef<K: 'static, A: 'static> {
    toolbar: ControlHandle,
    items: Vec<ToolbarItemRef<K>>,
    behavior: Rc<RefCell<ToolbarBehavior<K>>>,
    commands: Rc<Vec<ResolvedToolbarCommand<K, A>>>,
}

impl<K, A> ToolbarRef<K, A>
where
    K: Copy + Eq + Hash + 'static,
    A: 'static,
{
    pub const fn node(&self) -> UiNodeId {
        self.toolbar.node
    }

    pub fn items(&self) -> &[ToolbarItemRef<K>] {
        &self.items
    }

    pub fn active_command(&self) -> Option<K> {
        self.behavior.borrow().active_command()
    }

    pub fn navigate(
        &self,
        command: CompositeNavigationCommand,
        direction: WritingDirection,
    ) -> Result<ToolbarTransition<K>, CompositeError<K>> {
        self.behavior.borrow_mut().navigate(command, direction)
    }

    pub fn invoke_active(
        &self,
        source: ChangeSource,
    ) -> Result<ToolbarInvocation<K, A>, ToolbarInvocationError<K>> {
        let request = self
            .behavior
            .borrow_mut()
            .request_active_command(source)
            .map_err(ToolbarInvocationError::Composite)?;
        invoke_command(&self.commands, request.command, request.source)
    }

    pub const fn style(&self) -> Property<BoxStyle> {
        self.toolbar.style
    }
}

impl<K: Clone + 'static, A: 'static> Clone for ToolbarRef<K, A> {
    fn clone(&self) -> Self {
        Self {
            toolbar: self.toolbar,
            items: self.items.clone(),
            behavior: self.behavior.clone(),
            commands: self.commands.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ToolbarItemRef<K> {
    command: K,
    control: ControlHandle,
    enabled: bool,
    checked: Option<CheckState>,
}

impl<K: Copy> ToolbarItemRef<K> {
    pub const fn command(self) -> K {
        self.command
    }

    pub const fn node(self) -> UiNodeId {
        self.control.node
    }

    pub const fn is_enabled(self) -> bool {
        self.enabled
    }

    pub const fn checked(self) -> Option<CheckState> {
        self.checked
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarError<K> {
    MissingAccessibleName,
    Empty,
    DuplicateCommand(K),
    OwnerMismatch {
        expected: ComponentId,
        actual: ComponentId,
    },
}

impl<K: fmt::Debug> fmt::Display for ToolbarError<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingAccessibleName => formatter.write_str("toolbar accessible name is empty"),
            Self::Empty => formatter.write_str("toolbar has no commands"),
            Self::DuplicateCommand(command) => {
                write!(formatter, "toolbar repeats command {command:?}")
            }
            Self::OwnerMismatch { expected, actual } => write!(
                formatter,
                "toolbar command belongs to {actual:?}, expected owner {expected:?}"
            ),
        }
    }
}

impl<K: fmt::Debug> std::error::Error for ToolbarError<K> {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ToolbarInvocationError<K> {
    Composite(CompositeError<K>),
    MissingCommand(K),
    DisabledCommand(K),
}

impl<K: fmt::Debug> fmt::Display for ToolbarInvocationError<K> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "toolbar invocation failed: {self:?}")
    }
}

impl<K: fmt::Debug> std::error::Error for ToolbarInvocationError<K> {}

fn invoke_command<K, A>(
    commands: &[ResolvedToolbarCommand<K, A>],
    key: K,
    source: ChangeSource,
) -> Result<ToolbarInvocation<K, A>, ToolbarInvocationError<K>>
where
    K: Copy + Eq + 'static,
    A: 'static,
{
    let Some(command) = commands.iter().find(|command| *command.command.id() == key) else {
        return Err(ToolbarInvocationError::MissingCommand(key));
    };
    if !command.state.enabled() {
        return Err(ToolbarInvocationError::DisabledCommand(key));
    }
    let checked = command.state.checked();
    let Some(action) = command.command.invoke(command.state, source).into_action() else {
        return Err(ToolbarInvocationError::DisabledCommand(key));
    };
    Ok(ToolbarInvocation {
        command: key,
        action,
        source,
        checked,
    })
}

const fn check_state_semantic(state: CheckState) -> SemanticCheckState {
    match state {
        CheckState::Unchecked => SemanticCheckState::Unchecked,
        CheckState::Checked => SemanticCheckState::Checked,
        CheckState::Mixed => SemanticCheckState::Mixed,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use crate::runtime::{
        Component, ComponentRuntimeDriver, CreateContext, NoAction, Read, State, UpdateContext,
        ViewRuntime,
    };
    use crate::ui::{SemanticAction, UiRoot};

    use crate::application_components::ActionFactory;

    use super::*;

    struct ReadCapture {
        captured: Rc<Cell<Option<Read<bool>>>>,
    }

    impl Component for ReadCapture {
        type State = State<bool>;
        type Action = NoAction;

        fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
            let state = context.state(true);
            self.captured.set(Some(state.read()));
            state
        }

        fn mount(&self, _state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
            ui.foundation()
                .root(BoxStyle::default(), LayoutStyle::default(), |_| {})
        }

        fn action(
            &self,
            _state: &mut Self::State,
            action: Self::Action,
            _context: &mut UpdateContext<'_, Self>,
        ) {
            match action {}
        }
    }

    fn capture_read() -> (Read<bool>, ViewRuntime<ComponentRuntimeDriver<ReadCapture>>) {
        let captured = Rc::new(Cell::new(None));
        let runtime = ViewRuntime::from_component(ReadCapture {
            captured: captured.clone(),
        })
        .unwrap();
        (captured.get().unwrap(), runtime)
    }

    fn command(id: u32, enabled: Read<bool>) -> CommandSpec<u32, ()> {
        CommandSpec::new(
            id,
            format!("Command {id}"),
            enabled,
            ActionFactory::new(enabled.owner(), |_| ()),
        )
        .unwrap()
    }

    #[test]
    fn construction_requires_name_commands_unique_ids_and_one_owner() {
        let (first, _first_runtime) = capture_read();
        let (foreign, _foreign_runtime) = capture_read();
        assert!(matches!(
            Toolbar::<u32, ()>::new(" ", [command(1, first)]),
            Err(ToolbarError::MissingAccessibleName)
        ));
        assert!(matches!(
            Toolbar::<u32, ()>::new("Edit", []),
            Err(ToolbarError::Empty)
        ));
        assert!(matches!(
            Toolbar::new("Edit", [command(1, first), command(1, first)]),
            Err(ToolbarError::DuplicateCommand(1))
        ));
        assert!(matches!(
            Toolbar::new("Edit", [command(1, first), command(2, foreign)]),
            Err(ToolbarError::OwnerMismatch { .. })
        ));
    }

    #[test]
    fn neutral_behavior_handles_orientation_rtl_home_end_and_disabled_discovery() {
        let items = [
            CompositeItem {
                key: 1_u32,
                enabled: true,
            },
            CompositeItem {
                key: 2,
                enabled: false,
            },
            CompositeItem {
                key: 3,
                enabled: true,
            },
        ];
        let mut horizontal =
            ToolbarBehavior::new(items, ToolbarNavigationPolicy::default()).unwrap();
        assert_eq!(horizontal.active_command(), Some(1));
        horizontal
            .navigate(
                CompositeNavigationCommand::Right,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(horizontal.active_command(), Some(2));
        assert_eq!(
            horizontal.request_active_command(ChangeSource::Keyboard),
            Err(CompositeError::ActiveDescendantDisabled(2))
        );
        horizontal
            .navigate(
                CompositeNavigationCommand::Right,
                WritingDirection::RightToLeft,
            )
            .unwrap();
        assert_eq!(horizontal.active_command(), Some(1));
        horizontal
            .navigate(
                CompositeNavigationCommand::End,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(horizontal.active_command(), Some(3));
        horizontal
            .navigate(
                CompositeNavigationCommand::Up,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(horizontal.active_command(), Some(3));

        let mut vertical = ToolbarBehavior::new(
            items,
            ToolbarNavigationPolicy {
                orientation: ToolbarOrientation::Vertical,
                ..ToolbarNavigationPolicy::default()
            },
        )
        .unwrap();
        vertical
            .navigate(
                CompositeNavigationCommand::Right,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(vertical.active_command(), Some(1));
        vertical
            .navigate(
                CompositeNavigationCommand::Down,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(vertical.active_command(), Some(2));
        vertical
            .navigate(
                CompositeNavigationCommand::Home,
                WritingDirection::LeftToRight,
            )
            .unwrap();
        assert_eq!(vertical.active_command(), Some(1));
    }

    #[derive(Debug, PartialEq, Eq)]
    struct NonCloneAction {
        command: u32,
        source: ChangeSource,
    }

    #[derive(Debug)]
    enum MountedAction {
        Invoked(ToolbarInvocation<u32, NonCloneAction>),
    }

    struct MountedToolbar {
        toolbar: Rc<RefCell<Option<ToolbarRef<u32, NonCloneAction>>>>,
        actions: Rc<RefCell<Vec<NonCloneAction>>>,
    }

    struct MountedToolbarState {
        toolbar: Toolbar<u32, NonCloneAction>,
        _first_enabled: State<bool>,
        _second_enabled: State<bool>,
        _third_enabled: State<bool>,
        _third_checked: State<CheckState>,
    }

    impl Component for MountedToolbar {
        type State = MountedToolbarState;
        type Action = MountedAction;

        fn create(&self, context: &mut CreateContext<'_>) -> Self::State {
            let first_enabled = context.state(true);
            let second_enabled = context.state(false);
            let third_enabled = context.state(true);
            let third_checked = context.state(CheckState::Mixed);
            let owner = context.component();
            let first = CommandSpec::new(
                1,
                "Cut",
                first_enabled.read(),
                ActionFactory::new(owner, |source| NonCloneAction { command: 1, source }),
            )
            .unwrap();
            let second = CommandSpec::new(
                2,
                "Copy",
                second_enabled.read(),
                ActionFactory::new(owner, |source| NonCloneAction { command: 2, source }),
            )
            .unwrap();
            let third = CommandSpec::new(
                3,
                "Bold",
                third_enabled.read(),
                ActionFactory::new(owner, |source| NonCloneAction { command: 3, source }),
            )
            .unwrap()
            .checked(third_checked.read())
            .unwrap();
            MountedToolbarState {
                toolbar: Toolbar::new("Editing", [first, second, third])
                    .unwrap()
                    .density(DensityMetrics::baseline(DensityClass::Touch)),
                _first_enabled: first_enabled,
                _second_enabled: second_enabled,
                _third_enabled: third_enabled,
                _third_checked: third_checked,
            }
        }

        fn mount(&self, state: &Self::State, ui: &mut Ui<'_, '_, Self::Action>) -> UiRoot {
            let root = ui
                .foundation()
                .root(BoxStyle::default(), LayoutStyle::default(), |_| {});
            let toolbar = state
                .toolbar
                .mount(ui, root.0, MountedAction::Invoked)
                .unwrap();
            self.toolbar.replace(Some(toolbar));
            root
        }

        fn action(
            &self,
            _state: &mut Self::State,
            action: Self::Action,
            _context: &mut UpdateContext<'_, Self>,
        ) {
            match action {
                MountedAction::Invoked(invocation) => {
                    self.actions.borrow_mut().push(invocation.into_action())
                }
            }
        }
    }

    struct Harness {
        runtime: ViewRuntime<ComponentRuntimeDriver<MountedToolbar>>,
        toolbar: Rc<RefCell<Option<ToolbarRef<u32, NonCloneAction>>>>,
        actions: Rc<RefCell<Vec<NonCloneAction>>>,
    }

    fn mounted() -> Harness {
        let toolbar = Rc::new(RefCell::new(None));
        let actions = Rc::new(RefCell::new(Vec::new()));
        let runtime = ViewRuntime::from_component(MountedToolbar {
            toolbar: toolbar.clone(),
            actions: actions.clone(),
        })
        .unwrap();
        Harness {
            runtime,
            toolbar,
            actions,
        }
    }

    #[test]
    fn mounted_toolbar_has_one_focus_stop_item_semantics_and_density_floor() {
        let harness = mounted();
        let toolbar = harness.toolbar.borrow();
        let toolbar = toolbar.as_ref().unwrap();
        assert!(
            harness
                .runtime
                .ui()
                .interactions
                .get(toolbar.node())
                .unwrap()
                .focusable
        );
        assert!(toolbar.items().iter().all(|item| {
            !harness
                .runtime
                .ui()
                .interactions
                .get(item.node())
                .is_some_and(|interaction| interaction.focusable)
        }));
        let semantics = harness.runtime.ui().semantics.get(toolbar.node()).unwrap();
        assert_eq!(semantics.role, SemanticRole::Toolbar);
        assert_eq!(semantics.relationships.len(), 4);
        assert_eq!(
            semantics.relationships.last().unwrap().kind,
            SemanticRelationshipKind::ActiveDescendant
        );
        assert!(semantics.actions.contains(SemanticAction::Focus));

        let disabled = harness
            .runtime
            .ui()
            .semantics
            .get(toolbar.items()[1].node())
            .unwrap();
        assert!(disabled.state.disabled);
        assert!(disabled.effective_actions().is_empty());
        let checked = harness
            .runtime
            .ui()
            .semantics
            .get(toolbar.items()[2].node())
            .unwrap();
        assert_eq!(checked.state.checked, Some(SemanticCheckState::Mixed));
        assert_eq!(
            harness
                .runtime
                .ui()
                .box_styles
                .get(toolbar.items()[0].node())
                .unwrap()
                .min_size,
            SizeRule2D {
                width: SizeRule::Px(44.0),
                height: SizeRule::Px(44.0),
            }
        );
    }

    #[test]
    fn mounted_navigation_and_routes_preserve_sources_without_clone_actions() {
        let mut harness = mounted();
        let (root, disabled, third) = {
            let toolbar = harness.toolbar.borrow();
            let toolbar = toolbar.as_ref().unwrap();
            (
                toolbar.node(),
                toolbar.items()[1].node(),
                toolbar.items()[2].node(),
            )
        };
        {
            let toolbar = harness.toolbar.borrow();
            let toolbar = toolbar.as_ref().unwrap();
            toolbar
                .navigate(
                    CompositeNavigationCommand::Right,
                    WritingDirection::LeftToRight,
                )
                .unwrap();
            assert_eq!(toolbar.active_command(), Some(2));
            assert!(matches!(
                toolbar.invoke_active(ChangeSource::Accessibility),
                Err(ToolbarInvocationError::Composite(
                    CompositeError::ActiveDescendantDisabled(2)
                ))
            ));
            toolbar
                .navigate(
                    CompositeNavigationCommand::Right,
                    WritingDirection::LeftToRight,
                )
                .unwrap();
            let direct = toolbar.invoke_active(ChangeSource::Accessibility).unwrap();
            assert_eq!(direct.command(), &3);
            assert_eq!(direct.source(), ChangeSource::Accessibility);
            assert_eq!(direct.checked(), Some(CheckState::Mixed));
            assert_eq!(
                direct.into_action(),
                NonCloneAction {
                    command: 3,
                    source: ChangeSource::Accessibility,
                }
            );
        }
        assert!(
            !harness
                .runtime
                .dispatch_activation(disabled, ChangeSource::Pointer)
        );
        assert!(
            harness
                .runtime
                .dispatch_activation(third, ChangeSource::Pointer)
        );
        assert!(
            harness
                .runtime
                .dispatch_activation(root, ChangeSource::Programmatic)
        );
        assert_eq!(
            &*harness.actions.borrow(),
            &[
                NonCloneAction {
                    command: 3,
                    source: ChangeSource::Pointer,
                },
                NonCloneAction {
                    command: 3,
                    source: ChangeSource::Programmatic,
                },
            ]
        );
    }
}
