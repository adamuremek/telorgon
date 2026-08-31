//! Deterministic Theme v4 archive boundary. No legacy archive is imported.

use crate::theme::{CompiledTheme, ThemeError, ThemeResult};

pub const THEME_ARCHIVE_MAGIC: [u8; 4] = *b"LTH4";

impl CompiledTheme {
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(32 + self.styles.len() * 128);
        bytes.extend_from_slice(&THEME_ARCHIVE_MAGIC);
        bytes.extend_from_slice(&self.domain.id().0.to_le_bytes());
        bytes.extend_from_slice(&self.fingerprint.to_le_bytes());
        bytes.extend_from_slice(&(self.styles.len() as u32).to_le_bytes());
        for (name, id) in &self.names {
            bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&id.domain.0.to_le_bytes());
            bytes.extend_from_slice(&id.component.to_le_bytes());
            bytes.extend_from_slice(&id.style.to_le_bytes());
            let style = &self.styles[id];
            let payload = format!("{style:?}");
            bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            bytes.extend_from_slice(payload.as_bytes());
        }
        bytes
    }
}

pub fn validate_archive_header(bytes: &[u8]) -> ThemeResult<()> {
    match bytes.get(..4) {
        Some(magic) if magic == THEME_ARCHIVE_MAGIC => Ok(()),
        Some(b"LTH3") | Some(b"LTH2") => Err(ThemeError::new(
            "legacy theme archives are not supported; rebuild as LTH4",
        )),
        _ => Err(ThemeError::new("invalid Theme v4 archive magic")),
    }
}

#[cfg(test)]
mod tests {
    use crate::theme::{ThemeCatalog, ThemeDomain, ThemeSource};

    use super::*;

    #[test]
    fn archive_is_deterministic_lth4_and_legacy_magic_is_rejected() {
        let source = ThemeSource::parse("format='v4'\ndomain='application'").unwrap();
        let theme =
            CompiledTheme::compile(&source, &ThemeCatalog::new(ThemeDomain::Application)).unwrap();
        assert_eq!(theme.encode(), theme.encode());
        assert_eq!(&theme.encode()[..4], &THEME_ARCHIVE_MAGIC);
        assert!(validate_archive_header(b"LTH3old").is_err());
    }
}
