//! Typed application, shell, and preview theme namespace identities.

use crate::ui::ThemeScopeId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeDomain {
    Application,
    Shell,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThemeScopeKind {
    Root,
    Preview,
}

/// Domain-qualified scope identity used by the final resolver surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ThemeScope {
    id: ThemeScopeId,
    domain: ThemeDomain,
    kind: ThemeScopeKind,
}

impl ThemeScope {
    pub(crate) const fn new(id: ThemeScopeId, domain: ThemeDomain, kind: ThemeScopeKind) -> Self {
        Self { id, domain, kind }
    }

    pub const fn id(self) -> ThemeScopeId {
        self.id
    }

    pub const fn domain(self) -> ThemeDomain {
        self.domain
    }

    pub const fn kind(self) -> ThemeScopeKind {
        self.kind
    }
}
