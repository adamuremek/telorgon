//! Domain-owned component contracts and stable style/slot identities.

use std::collections::{BTreeMap, BTreeSet};

use crate::ui::{
    ComponentStyleId, InteractionFlags, StylePropertyPatch, StyleSlotId, ThemeDomainId,
};

use crate::theme::{ThemeDomain, ThemeError, ThemeResult};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StylePropertyMask(u32);

impl StylePropertyMask {
    pub const BOX: Self = Self(1 << 0);
    pub const TEXT: Self = Self(1 << 1);
    pub const TRANSFORM: Self = Self(1 << 2);
    pub const ALL: Self = Self(Self::BOX.0 | Self::TEXT.0 | Self::TRANSFORM.0);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InteractionState {
    Hovered,
    Pressed,
    Focused,
    FocusVisible,
    Disabled,
    ReadOnly,
    Busy,
    Checked,
    Mixed,
    Selected,
    Expanded,
    Invalid,
    Active,
    Highlighted,
    Dragging,
    Scrolling,
}

impl InteractionState {
    pub const ALL: [Self; 16] = [
        Self::Hovered,
        Self::Pressed,
        Self::Focused,
        Self::FocusVisible,
        Self::Disabled,
        Self::ReadOnly,
        Self::Busy,
        Self::Checked,
        Self::Mixed,
        Self::Selected,
        Self::Expanded,
        Self::Invalid,
        Self::Active,
        Self::Highlighted,
        Self::Dragging,
        Self::Scrolling,
    ];

    pub const fn flag(self) -> InteractionFlags {
        match self {
            Self::Hovered => InteractionFlags::HOVERED,
            Self::Pressed => InteractionFlags::PRESSED,
            Self::Focused => InteractionFlags::FOCUSED,
            Self::FocusVisible => InteractionFlags::FOCUS_VISIBLE,
            Self::Disabled => InteractionFlags::DISABLED,
            Self::ReadOnly => InteractionFlags::READ_ONLY,
            Self::Busy => InteractionFlags::BUSY,
            Self::Checked => InteractionFlags::CHECKED,
            Self::Mixed => InteractionFlags::MIXED,
            Self::Selected => InteractionFlags::SELECTED,
            Self::Expanded => InteractionFlags::EXPANDED,
            Self::Invalid => InteractionFlags::INVALID,
            Self::Active => InteractionFlags::ACTIVE,
            Self::Highlighted => InteractionFlags::HIGHLIGHTED,
            Self::Dragging => InteractionFlags::DRAGGING,
            Self::Scrolling => InteractionFlags::SCROLLING,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Hovered => "hovered",
            Self::Pressed => "pressed",
            Self::Focused => "focused",
            Self::FocusVisible => "focus-visible",
            Self::Disabled => "disabled",
            Self::ReadOnly => "read-only",
            Self::Busy => "busy",
            Self::Checked => "checked",
            Self::Mixed => "mixed",
            Self::Selected => "selected",
            Self::Expanded => "expanded",
            Self::Invalid => "invalid",
            Self::Active => "active",
            Self::Highlighted => "highlighted",
            Self::Dragging => "dragging",
            Self::Scrolling => "scrolling",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.name() == name)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleSlotContract {
    pub id: StyleSlotId,
    pub name: String,
    pub properties: StylePropertyMask,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CatalogStyle {
    pub id: ComponentStyleId,
    pub name: String,
    pub defaults: BTreeMap<StyleSlotId, StylePropertyPatch>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentStyleContract {
    pub component: String,
    pub slots: BTreeMap<String, StyleSlotContract>,
    pub styles: BTreeMap<String, BTreeMap<String, StylePropertyPatch>>,
    pub variant_axes: BTreeMap<String, BTreeSet<String>>,
    pub relevant_states: InteractionFlags,
    /// Low-to-high overlay order. Source insertion order is never observed.
    pub state_precedence: Vec<InteractionState>,
}

impl ComponentStyleContract {
    pub fn new(component: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            slots: BTreeMap::new(),
            styles: BTreeMap::new(),
            variant_axes: BTreeMap::new(),
            relevant_states: InteractionFlags::from_bits(u32::MAX),
            state_precedence: InteractionState::ALL.to_vec(),
        }
    }

    pub fn slot(mut self, name: impl Into<String>, properties: StylePropertyMask) -> Self {
        let name = name.into();
        self.slots.insert(
            name.clone(),
            StyleSlotContract {
                id: StyleSlotId::named(&name),
                name,
                properties,
            },
        );
        self
    }

    pub fn style(
        mut self,
        name: impl Into<String>,
        defaults: impl IntoIterator<Item = (String, StylePropertyPatch)>,
    ) -> Self {
        self.styles
            .insert(name.into(), defaults.into_iter().collect());
        self
    }

    pub fn variant_axis(
        mut self,
        axis: impl Into<String>,
        values: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.variant_axes
            .insert(axis.into(), values.into_iter().map(Into::into).collect());
        self
    }

    pub fn states(
        mut self,
        relevant: InteractionFlags,
        low_to_high: impl IntoIterator<Item = InteractionState>,
    ) -> Self {
        self.relevant_states = relevant;
        self.state_precedence = low_to_high.into_iter().collect();
        self
    }
}

#[derive(Clone, Debug)]
struct RegisteredContract {
    contract: ComponentStyleContract,
    styles: BTreeMap<String, CatalogStyle>,
}

#[derive(Clone, Debug)]
pub struct ThemeCatalog {
    domain: ThemeDomain,
    contracts: BTreeMap<String, RegisteredContract>,
    ids: BTreeMap<ComponentStyleId, (String, String)>,
    explicitly_unstyled: BTreeSet<String>,
}

impl ThemeCatalog {
    pub fn new(domain: ThemeDomain) -> Self {
        Self {
            domain,
            contracts: BTreeMap::new(),
            ids: BTreeMap::new(),
            explicitly_unstyled: BTreeSet::new(),
        }
    }

    pub const fn domain(&self) -> ThemeDomain {
        self.domain
    }

    pub fn register(&mut self, contract: ComponentStyleContract) -> ThemeResult<()> {
        validate_name(&contract.component, "component")?;
        if contract.slots.is_empty() || contract.styles.is_empty() {
            return Err(ThemeError::new(format!(
                "component `{}` must declare at least one slot and style",
                contract.component
            )));
        }
        if self.contracts.contains_key(&contract.component)
            || self.explicitly_unstyled.contains(&contract.component)
        {
            return Err(ThemeError::new(format!(
                "duplicate component `{}` in {} catalog",
                contract.component,
                self.domain.name()
            )));
        }
        let mut styles = BTreeMap::new();
        for (style_name, defaults) in &contract.styles {
            validate_name(style_name, "style")?;
            let id = ComponentStyleId::named(self.domain.id(), &contract.component, style_name);
            if self.ids.contains_key(&id) {
                return Err(ThemeError::new("stable component style ID collision"));
            }
            let mut compiled_defaults = BTreeMap::new();
            for (slot_name, patch) in defaults {
                let slot = contract.slots.get(slot_name).ok_or_else(|| {
                    ThemeError::new(format!(
                        "default for `{}` references unknown slot `{slot_name}`",
                        contract.component
                    ))
                })?;
                compiled_defaults.insert(slot.id, *patch);
            }
            styles.insert(
                style_name.clone(),
                CatalogStyle {
                    id,
                    name: style_name.clone(),
                    defaults: compiled_defaults,
                },
            );
            self.ids
                .insert(id, (contract.component.clone(), style_name.clone()));
        }
        self.contracts.insert(
            contract.component.clone(),
            RegisteredContract { contract, styles },
        );
        Ok(())
    }

    pub fn mark_unstyled(&mut self, component: impl Into<String>) -> ThemeResult<()> {
        let component = component.into();
        validate_name(&component, "component")?;
        if self.contracts.contains_key(&component) {
            return Err(ThemeError::new(format!(
                "visual component `{component}` already has a style contract"
            )));
        }
        self.explicitly_unstyled.insert(component);
        Ok(())
    }

    pub fn style_id(&self, component: &str, style: &str) -> Option<ComponentStyleId> {
        self.contracts
            .get(component)?
            .styles
            .get(style)
            .map(|item| item.id)
    }

    pub fn slot_id(&self, component: &str, slot: &str) -> Option<StyleSlotId> {
        self.contracts
            .get(component)?
            .contract
            .slots
            .get(slot)
            .map(|item| item.id)
    }

    pub(crate) fn contract(&self, component: &str) -> Option<&ComponentStyleContract> {
        self.contracts.get(component).map(|item| &item.contract)
    }

    pub(crate) fn catalog_style(&self, component: &str, style: &str) -> Option<&CatalogStyle> {
        self.contracts.get(component)?.styles.get(style)
    }

    pub fn contracts(&self) -> impl Iterator<Item = &ComponentStyleContract> {
        self.contracts.values().map(|item| &item.contract)
    }

    pub fn explicitly_unstyled(&self) -> impl Iterator<Item = &str> {
        self.explicitly_unstyled.iter().map(String::as_str)
    }
}

impl ThemeDomain {
    pub const fn id(self) -> ThemeDomainId {
        match self {
            Self::Application => ThemeDomainId::APPLICATION,
            Self::Shell => ThemeDomainId::SHELL,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Application => "application",
            Self::Shell => "shell",
        }
    }
}

fn validate_name(name: &str, kind: &str) -> ThemeResult<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(ThemeError::new(format!("invalid {kind} name `{name}`")))
    } else {
        Ok(())
    }
}

/// Catalog for the foundation node families present in every Telorgon view.
pub fn foundation_catalog(domain: ThemeDomain) -> ThemeCatalog {
    let mut catalog = ThemeCatalog::new(domain);
    for component in [
        "box",
        "text",
        "image",
        "button",
        "toggle",
        "text-input",
        "slider",
        "scroll",
        "collection",
        "custom",
    ] {
        let mut contract =
            ComponentStyleContract::new(component).slot("root", StylePropertyMask::ALL);
        contract = match component {
            "button" => contract.slot("label", StylePropertyMask::TEXT),
            "slider" => contract
                .slot("track", StylePropertyMask::BOX)
                .slot("fill", StylePropertyMask::BOX)
                .slot("thumb", StylePropertyMask::BOX)
                .slot("label", StylePropertyMask::TEXT),
            _ => contract,
        };
        catalog
            .register(
                contract
                    .style(
                        "default",
                        [("root".to_owned(), StylePropertyPatch::default())],
                    )
                    .states(
                        InteractionFlags::from_bits(u32::MAX),
                        [
                            InteractionState::Hovered,
                            InteractionState::Focused,
                            InteractionState::FocusVisible,
                            InteractionState::Checked,
                            InteractionState::Mixed,
                            InteractionState::Selected,
                            InteractionState::Expanded,
                            InteractionState::Active,
                            InteractionState::Highlighted,
                            InteractionState::Pressed,
                            InteractionState::Dragging,
                            InteractionState::Scrolling,
                            InteractionState::ReadOnly,
                            InteractionState::Busy,
                            InteractionState::Disabled,
                            InteractionState::Invalid,
                        ],
                    ),
            )
            .expect("built-in foundation contracts are unique and valid");
    }
    catalog
}

/// Shipped application visual contracts. Every first-party visual component or primitive must
/// occur here or in the foundation catalog.
pub const APPLICATION_VISUAL_COMPONENTS: &[&str] = &[
    "activity-indicator",
    "adaptive-scaffold",
    "application-region",
    "application-root",
    "breadcrumb",
    "checkbox",
    "data-grid",
    "hud-layer",
    "icon-button",
    "image-view",
    "label",
    "link",
    "list-box",
    "list-view",
    "menu",
    "meter",
    "navigation-bar",
    "navigation-rail",
    "progress",
    "radio",
    "range-slider",
    "render-target-view",
    "route-host",
    "scaffold",
    "scroll-view",
    "scrollbar",
    "separator",
    "split-view",
    "switch",
    "table",
    "tabs",
    "text-field",
    "toggle-button",
    "toolbar",
    "tree-view",
    "validation-summary",
    "video-surface",
    "viewport-overlay",
    "virtual-list",
    "world-anchor",
];

/// Shipped shell visual contracts, including shell-only primitives.
pub const SHELL_VISUAL_COMPONENTS: &[&str] = &[
    "application-grid",
    "client-surface",
    "dock",
    "exclusive-region",
    "floating-region",
    "launcher",
    "lock-composition",
    "notification-center",
    "notification-host",
    "on-screen-display",
    "output-view",
    "panel",
    "quick-settings",
    "shadow-frame",
    "shell-layer",
    "shell-root",
    "snap-preview",
    "start-menu",
    "status-area",
    "surface-placeholder",
    "surface-snapshot",
    "system-dialog",
    "system-modal-host",
    "taskbar",
    "tiling-region",
    "window-controls",
    "window-frame",
    "window-stack",
    "window-titlebar",
    "workspace-overview",
    "workspace-switcher",
    "workspace-view",
];

pub const APPLICATION_UNSTYLED_COMPONENTS: &[&str] = &[
    "application-overlay-controller",
    "command-model",
    "density-metrics",
    "selection-model",
];

pub const SHELL_UNSTYLED_COMPONENTS: &[&str] = &[
    "launcher-model",
    "notification-model",
    "panel-auto-hide-model",
    "workspace-model",
];

pub fn application_catalog() -> ThemeCatalog {
    extend_first_party_catalog(
        foundation_catalog(ThemeDomain::Application),
        APPLICATION_VISUAL_COMPONENTS,
        APPLICATION_UNSTYLED_COMPONENTS,
    )
}

pub fn shell_catalog() -> ThemeCatalog {
    extend_first_party_catalog(
        foundation_catalog(ThemeDomain::Shell),
        SHELL_VISUAL_COMPONENTS,
        SHELL_UNSTYLED_COMPONENTS,
    )
}

pub fn domain_catalog(domain: ThemeDomain) -> ThemeCatalog {
    match domain {
        ThemeDomain::Application => application_catalog(),
        ThemeDomain::Shell => shell_catalog(),
    }
}

fn extend_first_party_catalog(
    mut catalog: ThemeCatalog,
    visual: &[&str],
    unstyled: &[&str],
) -> ThemeCatalog {
    for component in visual {
        if catalog.style_id(component, "default").is_none() {
            let mut contract =
                ComponentStyleContract::new(*component).slot("root", StylePropertyMask::ALL);
            contract = match *component {
                "activity-indicator" => contract
                    .slot("track", StylePropertyMask::BOX)
                    .slot(
                        "marker",
                        StylePropertyMask::BOX.union(StylePropertyMask::TRANSFORM),
                    )
                    .slot("label", StylePropertyMask::TEXT),
                "checkbox" => contract
                    .slot("indicator", StylePropertyMask::BOX)
                    .slot("check-start", StylePropertyMask::BOX)
                    .slot("check-end", StylePropertyMask::BOX)
                    .slot("mixed", StylePropertyMask::BOX)
                    .slot("label", StylePropertyMask::TEXT),
                "switch" => contract
                    .slot("track", StylePropertyMask::BOX)
                    .slot("thumb", StylePropertyMask::BOX)
                    .slot("label", StylePropertyMask::TEXT),
                _ => contract,
            };
            catalog
                .register(contract.style(
                    "default",
                    [("root".to_owned(), StylePropertyPatch::default())],
                ))
                .expect("first-party visual component names are unique and valid");
        }
    }
    for component in unstyled {
        catalog
            .mark_unstyled(*component)
            .expect("first-party unstyled component names are unique and valid");
    }
    catalog
}

#[cfg(test)]
mod completeness_tests {
    use super::*;

    #[test]
    fn every_shipped_component_is_styled_or_explicitly_unstyled() {
        for (catalog, visual, unstyled) in [
            (
                application_catalog(),
                APPLICATION_VISUAL_COMPONENTS,
                APPLICATION_UNSTYLED_COMPONENTS,
            ),
            (
                shell_catalog(),
                SHELL_VISUAL_COMPONENTS,
                SHELL_UNSTYLED_COMPONENTS,
            ),
        ] {
            for component in visual {
                assert!(
                    catalog.style_id(component, "default").is_some(),
                    "missing visual contract for {component}"
                );
            }
            let declared_unstyled = catalog.explicitly_unstyled().collect::<BTreeSet<_>>();
            for component in unstyled {
                assert!(declared_unstyled.contains(component));
            }
        }
    }
}
