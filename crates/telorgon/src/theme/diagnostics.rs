//! Structured compiler diagnostics retained separately from runtime resolution.

/// One deterministic source-normalization diagnostic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeDiagnostic {
    pub style: String,
    pub message: String,
}

impl ThemeDiagnostic {
    pub fn new(style: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            style: style.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeRuntimeDiagnostics {
    pub bindings_evaluated: u64,
    pub bindings_skipped: u64,
    pub entries_invalidated: u64,
    pub active_animations: u64,
    pub retargets: u64,
    pub stale_controls_rejected: u64,
    pub stale_scopes_rejected: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThemeUpdate {
    pub changed: bool,
    pub active_animations: bool,
    pub diagnostics: ThemeRuntimeDiagnostics,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_retains_only_the_explicit_compiler_fields() {
        let diagnostic = ThemeDiagnostic::new("button", "normalized legacy radius");
        assert_eq!(diagnostic.style, "button");
        assert_eq!(diagnostic.message, "normalized legacy radius");
    }
}
