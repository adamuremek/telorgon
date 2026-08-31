//! Focused theme compilation and source error boundary.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeError {
    message: String,
}

impl ThemeError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ThemeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ThemeError {}

pub type ThemeResult<T> = Result<T, ThemeError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_preserves_the_existing_display_contract() {
        let error = ThemeError::new("invalid theme");
        assert_eq!(error.to_string(), "invalid theme");
        assert_eq!(error, error.clone());
    }
}
