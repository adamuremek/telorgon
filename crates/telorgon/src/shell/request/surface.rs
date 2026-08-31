//! Typed surface-policy intentions emitted by shell UI.

use crate::shell::{ContactId, InputSource, ShellCapabilities, SurfaceCapabilities, SurfaceId};

/// Logical edge or corner used to begin an interactive surface resize.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResizeEdge {
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
}

/// A shell request concerning one host-owned client surface.
///
/// Constructing a value does not mutate a [`crate::shell::ClientSurfaceSnapshot`]. The policy host checks
/// the current grant, surface capability, identity, revision, input causality, and session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SurfaceRequest {
    Activate {
        surface: SurfaceId,
        source: InputSource,
    },
    Close {
        surface: SurfaceId,
    },
    BeginMove {
        surface: SurfaceId,
        contact: ContactId,
    },
    BeginResize {
        surface: SurfaceId,
        edge: ResizeEdge,
        contact: ContactId,
    },
    SetMinimized {
        surface: SurfaceId,
        minimized: bool,
    },
    SetMaximized {
        surface: SurfaceId,
        maximized: bool,
    },
    SetFullscreen {
        surface: SurfaceId,
        fullscreen: bool,
    },
}

impl SurfaceRequest {
    pub const fn surface(self) -> SurfaceId {
        match self {
            Self::Activate { surface, .. }
            | Self::Close { surface }
            | Self::BeginMove { surface, .. }
            | Self::BeginResize { surface, .. }
            | Self::SetMinimized { surface, .. }
            | Self::SetMaximized { surface, .. }
            | Self::SetFullscreen { surface, .. } => surface,
        }
    }

    pub const fn required_shell_capability(self) -> ShellCapabilities {
        match self {
            Self::Activate { .. } => ShellCapabilities::ACTIVATE_SURFACE,
            Self::Close { .. } => ShellCapabilities::CLOSE_SURFACE,
            Self::BeginMove { .. } => ShellCapabilities::MOVE_SURFACE,
            Self::BeginResize { .. } => ShellCapabilities::RESIZE_SURFACE,
            Self::SetMinimized { .. } => ShellCapabilities::MINIMIZE_SURFACE,
            Self::SetMaximized { .. } => ShellCapabilities::MAXIMIZE_SURFACE,
            Self::SetFullscreen { .. } => ShellCapabilities::FULLSCREEN_SURFACE,
        }
    }

    pub const fn required_surface_capability(self) -> SurfaceCapabilities {
        match self {
            Self::Activate { .. } => SurfaceCapabilities::ACTIVATE,
            Self::Close { .. } => SurfaceCapabilities::CLOSE,
            Self::BeginMove { .. } => SurfaceCapabilities::MOVE,
            Self::BeginResize { .. } => SurfaceCapabilities::RESIZE,
            Self::SetMinimized { .. } => SurfaceCapabilities::MINIMIZE,
            Self::SetMaximized { .. } => SurfaceCapabilities::MAXIMIZE,
            Self::SetFullscreen { .. } => SurfaceCapabilities::FULLSCREEN,
        }
    }

    pub const fn input_source(self) -> Option<InputSource> {
        match self {
            Self::Activate { source, .. } => Some(source),
            _ => None,
        }
    }

    pub const fn contact(self) -> Option<ContactId> {
        match self {
            Self::BeginMove { contact, .. } | Self::BeginResize { contact, .. } => Some(contact),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn surface() -> SurfaceId {
        SurfaceId::from_raw(97).unwrap()
    }

    #[test]
    fn every_request_exposes_its_target_and_exact_capability_pair() {
        let contact = ContactId::from_raw(2).unwrap();
        let requests = [
            (
                SurfaceRequest::Activate {
                    surface: surface(),
                    source: InputSource::Keyboard,
                },
                ShellCapabilities::ACTIVATE_SURFACE,
                SurfaceCapabilities::ACTIVATE,
            ),
            (
                SurfaceRequest::Close { surface: surface() },
                ShellCapabilities::CLOSE_SURFACE,
                SurfaceCapabilities::CLOSE,
            ),
            (
                SurfaceRequest::BeginMove {
                    surface: surface(),
                    contact,
                },
                ShellCapabilities::MOVE_SURFACE,
                SurfaceCapabilities::MOVE,
            ),
            (
                SurfaceRequest::BeginResize {
                    surface: surface(),
                    edge: ResizeEdge::BottomRight,
                    contact,
                },
                ShellCapabilities::RESIZE_SURFACE,
                SurfaceCapabilities::RESIZE,
            ),
            (
                SurfaceRequest::SetMinimized {
                    surface: surface(),
                    minimized: true,
                },
                ShellCapabilities::MINIMIZE_SURFACE,
                SurfaceCapabilities::MINIMIZE,
            ),
            (
                SurfaceRequest::SetMaximized {
                    surface: surface(),
                    maximized: true,
                },
                ShellCapabilities::MAXIMIZE_SURFACE,
                SurfaceCapabilities::MAXIMIZE,
            ),
            (
                SurfaceRequest::SetFullscreen {
                    surface: surface(),
                    fullscreen: true,
                },
                ShellCapabilities::FULLSCREEN_SURFACE,
                SurfaceCapabilities::FULLSCREEN,
            ),
        ];

        for (request, shell_capability, surface_capability) in requests {
            assert_eq!(request.surface(), surface());
            assert_eq!(request.required_shell_capability(), shell_capability);
            assert_eq!(request.required_surface_capability(), surface_capability);
        }
    }

    #[test]
    fn causality_metadata_is_retained_only_by_applicable_requests() {
        let contact = ContactId::from_raw(3).unwrap();
        let resize = SurfaceRequest::BeginResize {
            surface: surface(),
            edge: ResizeEdge::TopLeft,
            contact,
        };
        let activate = SurfaceRequest::Activate {
            surface: surface(),
            source: InputSource::Accessibility,
        };

        assert_eq!(resize.contact(), Some(contact));
        assert_eq!(resize.input_source(), None);
        assert_eq!(activate.contact(), None);
        assert_eq!(activate.input_source(), Some(InputSource::Accessibility));
    }
}
