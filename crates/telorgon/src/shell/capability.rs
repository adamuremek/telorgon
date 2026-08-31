//! Host-granted shell operations and output-scoped layer authority.

use std::fmt;
use std::num::NonZeroU64;

use crate::shell::OutputId;

/// Opaque identity of a capability grant issued by the policy host.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ShellGrantToken(NonZeroU64);

impl ShellGrantToken {
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    pub const fn from_raw(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Operations and layer classes that a host grant permits.
///
/// This is an assertion supplied by a trusted host boundary, not a security boundary by itself.
/// Every request still requires host-side identity, revision, session, and policy validation.
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ShellCapabilities(u32);

impl ShellCapabilities {
    pub const NONE: Self = Self(0);

    pub const ACTIVATE_SURFACE: Self = Self(1 << 0);
    pub const CLOSE_SURFACE: Self = Self(1 << 1);
    pub const MOVE_SURFACE: Self = Self(1 << 2);
    pub const RESIZE_SURFACE: Self = Self(1 << 3);
    pub const MINIMIZE_SURFACE: Self = Self(1 << 4);
    pub const SELECT_WORKSPACE: Self = Self(1 << 5);
    pub const MANAGE_WORKSPACES: Self = Self(1 << 6);
    pub const RESERVE_OUTPUT_AREA: Self = Self(1 << 7);
    pub const CONFIGURE_OUTPUT: Self = Self(1 << 8);
    pub const INVOKE_SYSTEM_ACTION: Self = Self(1 << 9);
    pub const FORWARD_CLIENT_INPUT: Self = Self(1 << 10);
    pub const RETAIN_SURFACE_SNAPSHOT: Self = Self(1 << 11);
    pub const MAXIMIZE_SURFACE: Self = Self(1 << 12);
    pub const FULLSCREEN_SURFACE: Self = Self(1 << 13);

    pub const BACKGROUND_LAYER: Self = Self(1 << 16);
    pub const WORKSPACE_LAYER: Self = Self(1 << 17);
    pub const PANEL_LAYER: Self = Self(1 << 18);
    pub const OVERLAY_LAYER: Self = Self(1 << 19);
    pub const SYSTEM_MODAL_LAYER: Self = Self(1 << 20);
    pub const LOCK_LAYER: Self = Self(1 << 21);
    pub const CURSOR_LAYER: Self = Self(1 << 22);

    const ALL_BITS: u32 = ((1 << 14) - 1) | (((1 << 7) - 1) << 16);

    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits & !Self::ALL_BITS == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl fmt::Debug for ShellCapabilities {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ShellCapabilities")
            .field("bits", &format_args!("{:#025b}", self.bits()))
            .finish()
    }
}

impl std::ops::BitOr for ShellCapabilities {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for ShellCapabilities {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Canonical back-to-front shell layer classes for one output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ShellLayerKind {
    Background,
    Workspace,
    Panel,
    Overlay,
    SystemModal,
    Lock,
    Cursor,
}

impl ShellLayerKind {
    pub const ALL: [Self; 7] = [
        Self::Background,
        Self::Workspace,
        Self::Panel,
        Self::Overlay,
        Self::SystemModal,
        Self::Lock,
        Self::Cursor,
    ];

    pub const fn required_capability(self) -> ShellCapabilities {
        match self {
            Self::Background => ShellCapabilities::BACKGROUND_LAYER,
            Self::Workspace => ShellCapabilities::WORKSPACE_LAYER,
            Self::Panel => ShellCapabilities::PANEL_LAYER,
            Self::Overlay => ShellCapabilities::OVERLAY_LAYER,
            Self::SystemModal => ShellCapabilities::SYSTEM_MODAL_LAYER,
            Self::Lock => ShellCapabilities::LOCK_LAYER,
            Self::Cursor => ShellCapabilities::CURSOR_LAYER,
        }
    }
}

/// Immutable host-issued capability grant scoped to one output.
///
/// `from_host` is deliberately the only public construction boundary. Shell roots retain grants;
/// less-trusted extensions receive only the narrower [`LayerAuthority`] values they need.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShellCapabilityGrant {
    token: ShellGrantToken,
    output: OutputId,
    capabilities: ShellCapabilities,
}

impl ShellCapabilityGrant {
    pub const fn from_host(
        token: ShellGrantToken,
        output: OutputId,
        capabilities: ShellCapabilities,
    ) -> Self {
        Self {
            token,
            output,
            capabilities,
        }
    }

    pub const fn token(self) -> ShellGrantToken {
        self.token
    }

    pub const fn output(self) -> OutputId {
        self.output
    }

    pub const fn capabilities(self) -> ShellCapabilities {
        self.capabilities
    }

    pub const fn permits(self, capability: ShellCapabilities) -> bool {
        self.capabilities.contains(capability)
    }

    /// Narrows this grant into an unforgeable-by-enum layer token for a shell root to distribute.
    pub fn authorize_layer(
        self,
        layer: ShellLayerKind,
    ) -> Result<LayerAuthority, LayerAuthorityError> {
        if !self.permits(layer.required_capability()) {
            return Err(LayerAuthorityError::NotGranted { layer });
        }
        Ok(LayerAuthority {
            grant: self.token,
            output: self.output,
            layer,
        })
    }
}

/// Narrow proof that a host grant permits one layer class on one output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerAuthority {
    grant: ShellGrantToken,
    output: OutputId,
    layer: ShellLayerKind,
}

impl LayerAuthority {
    pub const fn grant(self) -> ShellGrantToken {
        self.grant
    }

    pub const fn output(self) -> OutputId {
        self.output
    }

    pub const fn layer(self) -> ShellLayerKind {
        self.layer
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerAuthorityError {
    NotGranted { layer: ShellLayerKind },
}

impl fmt::Display for LayerAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotGranted { layer } => {
                write!(formatter, "the host grant does not authorize {layer:?}")
            }
        }
    }
}

impl std::error::Error for LayerAuthorityError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn output() -> OutputId {
        OutputId::from_raw(7).unwrap()
    }

    #[test]
    fn unknown_capability_bits_are_rejected() {
        assert_eq!(ShellCapabilities::from_bits(1 << 15), None);
        assert_eq!(ShellCapabilities::from_bits(1 << 31), None);
    }

    #[test]
    fn a_grant_can_only_narrow_to_an_authorized_layer() {
        let grant = ShellCapabilityGrant::from_host(
            ShellGrantToken::from_raw(3).unwrap(),
            output(),
            ShellCapabilities::WORKSPACE_LAYER | ShellCapabilities::PANEL_LAYER,
        );

        let panel = grant.authorize_layer(ShellLayerKind::Panel).unwrap();
        assert_eq!(panel.output(), output());
        assert_eq!(panel.layer(), ShellLayerKind::Panel);
        assert_eq!(panel.grant(), grant.token());
        assert_eq!(
            grant.authorize_layer(ShellLayerKind::Lock),
            Err(LayerAuthorityError::NotGranted {
                layer: ShellLayerKind::Lock,
            })
        );
    }

    #[test]
    fn canonical_layer_order_is_back_to_front() {
        assert_eq!(ShellLayerKind::ALL[0], ShellLayerKind::Background);
        assert_eq!(ShellLayerKind::ALL[6], ShellLayerKind::Cursor);
        assert!(ShellLayerKind::Workspace < ShellLayerKind::Overlay);
        assert!(ShellLayerKind::SystemModal < ShellLayerKind::Lock);
    }
}
