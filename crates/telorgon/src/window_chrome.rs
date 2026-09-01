//! Protocol-neutral window metadata, chrome roles, actions, and layout-derived hit regions.

use crate::assets::Icon;
use crate::core::RectF;
use crate::layout::LayoutEngine;
use crate::render::ImageId;
use crate::ui::{MountedUi, UiNodeId};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WindowChromeState {
    #[default]
    Normal,
    Maximized,
    Fullscreen,
    Tiled,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowChromeCapabilities {
    pub close: bool,
    pub minimize: bool,
    pub maximize: bool,
    pub move_window: bool,
    pub resize: bool,
    pub system_menu: bool,
}

impl WindowChromeCapabilities {
    pub const MANAGED_TOPLEVEL: Self = Self {
        close: true,
        minimize: true,
        maximize: true,
        move_window: true,
        resize: true,
        system_menu: true,
    };
}

/// Immutable input supplied to one compositor-owned frame composition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowChromeModel {
    pub window_id: u64,
    pub title: String,
    pub app_icon: Option<Icon>,
    pub app_icon_name: Option<String>,
    pub app_icon_image: Option<ImageId>,
    pub state: WindowChromeState,
    pub active: bool,
    pub capabilities: WindowChromeCapabilities,
}

impl WindowChromeModel {
    pub fn new(window_id: u64, title: impl Into<String>) -> Self {
        Self {
            window_id,
            title: title.into(),
            app_icon: None,
            app_icon_name: None,
            app_icon_image: None,
            state: WindowChromeState::Normal,
            active: false,
            capabilities: WindowChromeCapabilities::MANAGED_TOPLEVEL,
        }
    }

    pub const fn app_icon(mut self, icon: Icon) -> Self {
        self.app_icon = Some(icon);
        self
    }

    pub fn app_icon_name(mut self, name: impl Into<String>) -> Self {
        self.app_icon_name = Some(name.into());
        self
    }

    pub const fn app_icon_image(mut self, image: ImageId) -> Self {
        self.app_icon_image = Some(image);
        self
    }

    pub const fn state(mut self, state: WindowChromeState) -> Self {
        self.state = state;
        self
    }

    pub const fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub const fn capabilities(mut self, capabilities: WindowChromeCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowResizeEdge {
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowAction {
    Close,
    Minimize,
    ToggleMaximize,
    BeginMove,
    BeginResize(WindowResizeEdge),
    ShowSystemMenu,
}

/// Semantic role attached to a composed frame node; geometry remains owned by normal layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowChromeRole {
    Frame,
    Content,
    Title,
    AppIcon,
    DragRegion,
    Action(WindowAction),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowChromeRegion {
    pub node: UiNodeId,
    pub role: WindowChromeRole,
    pub bounds: RectF,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WindowChromeSnapshot {
    pub frame: WindowChromeRegion,
    pub content: WindowChromeRegion,
    pub regions: Vec<WindowChromeRegion>,
}

impl WindowChromeSnapshot {
    pub fn derive(ui: &MountedUi, layout: &LayoutEngine) -> Result<Self, WindowChromeError> {
        let mut frame = None;
        let mut content = None;
        let mut regions = Vec::new();
        for (node, role) in ui.window_chrome_roles.iter() {
            let Some(computed) = layout.computed(node) else {
                continue;
            };
            let region = WindowChromeRegion {
                node,
                role: *role,
                bounds: computed.border_rect,
            };
            match role {
                WindowChromeRole::Frame => {
                    if frame.replace(region).is_some() {
                        return Err(WindowChromeError::MultipleFrames);
                    }
                }
                WindowChromeRole::Content => {
                    if content.replace(region).is_some() {
                        return Err(WindowChromeError::MultipleContentSlots);
                    }
                }
                _ => regions.push(region),
            }
        }
        Ok(Self {
            frame: frame.ok_or(WindowChromeError::MissingFrame)?,
            content: content.ok_or(WindowChromeError::MissingContentSlot)?,
            regions,
        })
    }

    /// Returns the top-most action/drag region containing a frame-local point.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<WindowChromeRole> {
        let point = crate::core::PointF { x, y };
        let containing = || {
            self.regions
                .iter()
                .rev()
                .filter(|region| region.bounds.contains(point))
        };
        containing()
            .find(|region| matches!(region.role, WindowChromeRole::Action(_)))
            .or_else(|| containing().find(|region| region.role == WindowChromeRole::DragRegion))
            .or_else(|| containing().next())
            .map(|region| region.role)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WindowChromeError {
    #[error("window frame composition has no frame root")]
    MissingFrame,
    #[error("window frame composition has multiple frame roots")]
    MultipleFrames,
    #[error("window frame composition has no client content slot")]
    MissingContentSlot,
    #[error("window frame composition has multiple client content slots")]
    MultipleContentSlots,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::NodeId;

    fn region(index: u32, role: WindowChromeRole, bounds: RectF) -> WindowChromeRegion {
        WindowChromeRegion {
            node: NodeId::new(index, 1),
            role,
            bounds,
        }
    }

    #[test]
    fn nested_action_regions_take_priority_over_a_drag_parent() {
        let frame = region(
            0,
            WindowChromeRole::Frame,
            RectF {
                x: 0.0,
                y: 0.0,
                width: 320.0,
                height: 240.0,
            },
        );
        let content = region(1, WindowChromeRole::Content, frame.bounds);
        let snapshot = WindowChromeSnapshot {
            frame,
            content,
            regions: vec![
                region(
                    2,
                    WindowChromeRole::Action(WindowAction::Close),
                    RectF {
                        x: 280.0,
                        y: 0.0,
                        width: 40.0,
                        height: 40.0,
                    },
                ),
                region(
                    3,
                    WindowChromeRole::DragRegion,
                    RectF {
                        x: 0.0,
                        y: 0.0,
                        width: 320.0,
                        height: 40.0,
                    },
                ),
            ],
        };

        assert_eq!(
            snapshot.hit_test(300.0, 20.0),
            Some(WindowChromeRole::Action(WindowAction::Close))
        );
        assert_eq!(
            snapshot.hit_test(100.0, 20.0),
            Some(WindowChromeRole::DragRegion)
        );
    }
}
