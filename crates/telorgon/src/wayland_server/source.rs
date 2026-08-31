use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::wayland_server::protocol::{DESKTOP_PROTOCOLS, ProtocolSpec};
use crate::wayland_server::{ProtocolSchema, ProtocolSchemaError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolSourcePaths {
    pub wayland_xml: PathBuf,
    pub wayland_protocols_root: PathBuf,
}

impl ProtocolSourcePaths {
    pub fn standard_linux() -> Self {
        Self {
            wayland_xml: PathBuf::from("/usr/share/wayland/wayland.xml"),
            wayland_protocols_root: PathBuf::from("/usr/share/wayland-protocols"),
        }
    }

    fn resolve(&self, spec: &ProtocolSpec) -> PathBuf {
        if spec.name == "wayland" {
            self.wayland_xml.clone()
        } else {
            self.wayland_protocols_root.join(spec.source)
        }
    }
}

impl Default for ProtocolSourcePaths {
    fn default() -> Self {
        Self::standard_linux()
    }
}

#[derive(Clone, Debug)]
pub struct LoadedProtocol {
    pub profile: &'static ProtocolSpec,
    pub source_path: PathBuf,
    pub schema: ProtocolSchema,
}

#[derive(Clone, Debug)]
pub struct ProtocolCatalog {
    protocols: Vec<LoadedProtocol>,
}

impl ProtocolCatalog {
    pub fn load_desktop(paths: &ProtocolSourcePaths) -> Result<Self, ProtocolSourceError> {
        let mut protocols = Vec::with_capacity(DESKTOP_PROTOCOLS.len());
        for profile in DESKTOP_PROTOCOLS {
            let source_path = paths.resolve(profile);
            let source = fs::read_to_string(&source_path).map_err(|error| {
                ProtocolSourceError::read(profile.name, source_path.clone(), error)
            })?;
            let schema = ProtocolSchema::parse(&source).map_err(|error| {
                ProtocolSourceError::schema(profile.name, source_path.clone(), error)
            })?;
            validate_profile(profile, &schema, &source_path)?;
            protocols.push(LoadedProtocol {
                profile,
                source_path,
                schema,
            });
        }
        Ok(Self { protocols })
    }

    pub fn protocols(&self) -> &[LoadedProtocol] {
        &self.protocols
    }

    pub fn protocol(&self, name: &str) -> Option<&LoadedProtocol> {
        self.protocols
            .iter()
            .find(|protocol| protocol.profile.name == name)
    }

    pub fn merged_schema(&self) -> Result<ProtocolSchema, ProtocolSourceError> {
        let mut interfaces = Vec::new();
        for protocol in &self.protocols {
            for interface in &protocol.schema.interfaces {
                if interfaces
                    .iter()
                    .any(|candidate: &crate::wayland_server::InterfaceSchema| {
                        candidate.name == interface.name
                    })
                {
                    return Err(ProtocolSourceError::duplicate_interface(
                        interface.name.clone(),
                        protocol.source_path.clone(),
                    ));
                }
                interfaces.push(interface.clone());
            }
        }
        Ok(ProtocolSchema {
            name: "telorgon-desktop".to_owned(),
            interfaces,
        })
    }
}

fn validate_profile(
    profile: &'static ProtocolSpec,
    schema: &ProtocolSchema,
    path: &Path,
) -> Result<(), ProtocolSourceError> {
    for expected in profile.interfaces {
        let Some(interface) = schema.interface(expected.name) else {
            return Err(ProtocolSourceError::profile(
                profile.name,
                path.to_owned(),
                format!("missing interface {}", expected.name),
            ));
        };
        if interface.version < expected.source_version {
            return Err(ProtocolSourceError::profile(
                profile.name,
                path.to_owned(),
                format!(
                    "interface {} has source version {}, profile requires {}",
                    expected.name, interface.version, expected.source_version
                ),
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct ProtocolSourceError {
    protocol: String,
    path: PathBuf,
    detail: String,
}

impl ProtocolSourceError {
    fn read(protocol: &str, path: PathBuf, error: std::io::Error) -> Self {
        Self {
            protocol: protocol.to_owned(),
            path,
            detail: error.to_string(),
        }
    }

    fn schema(protocol: &str, path: PathBuf, error: ProtocolSchemaError) -> Self {
        Self {
            protocol: protocol.to_owned(),
            path,
            detail: error.to_string(),
        }
    }

    fn profile(protocol: &str, path: PathBuf, detail: String) -> Self {
        Self {
            protocol: protocol.to_owned(),
            path,
            detail,
        }
    }

    fn duplicate_interface(interface: String, path: PathBuf) -> Self {
        Self {
            protocol: "telorgon-desktop".to_owned(),
            path,
            detail: format!("duplicate interface {interface}"),
        }
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl fmt::Display for ProtocolSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not load Wayland protocol {} from {}: {}",
            self.protocol,
            self.path.display(),
            self.detail
        )
    }
}

impl std::error::Error for ProtocolSourceError {}
