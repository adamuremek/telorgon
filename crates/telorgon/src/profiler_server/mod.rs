//! Managed loopback service and embedded browser viewer for the Telorgon profiler.

mod protocol;
mod service;
mod store;

use std::ffi::OsStr;
use std::path::Path;

use serde::Serialize;

pub use service::{ProfilerServer, ServerError};

pub const PROFILER_ARGUMENT: &str = "--telorgon-profile";
const FIRST_STABLE_PORT: u16 = 42_000;
const STABLE_PORT_COUNT: u16 = 7_000;

/// Host entrypoint that owns the profiled session.
///
/// This is session metadata rather than an instrumentation mode: every target uses the same event
/// and capture protocol, and may publish any number of independently correlated views.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProfileTarget {
    #[default]
    Gui,
    DesktopEnvironment,
}

/// Parsed activation request for a managed process.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProfilerRequest {
    #[default]
    Disabled,
    Enabled,
}

impl ProfilerRequest {
    #[must_use]
    pub fn from_process_args() -> Self {
        Self::from_args(std::env::args_os())
    }

    #[must_use]
    pub fn from_args<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        if args
            .into_iter()
            .any(|argument| argument.as_ref() == OsStr::new(PROFILER_ARGUMENT))
        {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }
}

/// Bounded metadata attached to a live session and saved capture.
#[derive(Clone, Debug, Serialize)]
pub struct SessionMetadata {
    pub application: String,
    pub executable: String,
    pub entrypoint: ProfileTarget,
    pub build_profile: String,
    pub target_os: String,
    pub target_arch: String,
    pub renderer: String,
    pub git_revision: Option<String>,
    pub capabilities: Vec<String>,
    pub unavailable_metrics: Vec<String>,
}

impl SessionMetadata {
    #[must_use]
    pub fn discover() -> Self {
        Self::discover_for(ProfileTarget::Gui)
    }

    #[must_use]
    pub fn discover_for(entrypoint: ProfileTarget) -> Self {
        let executable = std::env::current_exe()
            .ok()
            .as_deref()
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .map_or_else(|| "telorgon-application".to_owned(), bounded_text);
        Self {
            application: executable.clone(),
            executable,
            entrypoint,
            build_profile: if cfg!(debug_assertions) {
                "debug-information-enabled".to_owned()
            } else {
                "optimized".to_owned()
            },
            target_os: std::env::consts::OS.to_owned(),
            target_arch: std::env::consts::ARCH.to_owned(),
            renderer: "managed selection pending".to_owned(),
            git_revision: option_env!("TELORGON_GIT_REVISION").map(bounded_text),
            capabilities: vec![
                "cpu-spans".to_owned(),
                "frame-counters".to_owned(),
                "capture-download".to_owned(),
            ],
            unavailable_metrics: vec!["gpu-relative-timestamps".to_owned()],
        }
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self {
            application: "fixture".to_owned(),
            executable: "fixture".to_owned(),
            entrypoint: ProfileTarget::Gui,
            build_profile: "test".to_owned(),
            target_os: "test".to_owned(),
            target_arch: "test".to_owned(),
            renderer: "test".to_owned(),
            git_revision: None,
            capabilities: Vec::new(),
            unavailable_metrics: Vec::new(),
        }
    }
}

fn bounded_text(value: &str) -> String {
    value.chars().take(128).collect()
}

/// Managed profiler startup configuration.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub metadata: SessionMetadata,
    /// Stable loopback port used so an already-open viewer can find a restarted application.
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            metadata: SessionMetadata::discover(),
            port: discover_port(),
        }
    }
}

impl ServerConfig {
    #[must_use]
    pub fn for_target(entrypoint: ProfileTarget) -> Self {
        Self {
            metadata: SessionMetadata::discover_for(entrypoint),
            port: discover_port(),
        }
    }
}

fn discover_port() -> u16 {
    if let Some(port) = std::env::var("TELORGON_PROFILER_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port != 0)
    {
        return port;
    }
    let identity = std::env::current_exe().ok().map_or_else(
        || "telorgon-application".to_owned(),
        |path| path.to_string_lossy().into_owned(),
    );
    stable_port_for(&identity)
}

fn stable_port_for(identity: &str) -> u16 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in identity.bytes() {
        hash ^= u64::from(byte.to_ascii_lowercase());
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    FIRST_STABLE_PORT + u16::try_from(hash % u64::from(STABLE_PORT_COUNT)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activation_requires_the_exact_reserved_argument() {
        assert_eq!(
            ProfilerRequest::from_args(["app", "--telorgon-profiled"]),
            ProfilerRequest::Disabled
        );
        assert_eq!(
            ProfilerRequest::from_args(["app", PROFILER_ARGUMENT]),
            ProfilerRequest::Enabled
        );
    }

    #[test]
    fn removed_browser_switch_does_not_activate_profiling() {
        assert_eq!(
            ProfilerRequest::from_args(["app", "--telorgon-profile-no-open"]),
            ProfilerRequest::Disabled
        );
    }

    #[test]
    fn stable_ports_are_deterministic_and_stay_in_the_configured_range() {
        let first = stable_port_for(r"C:\apps\counter\target\telorgon-profile\counter.exe");
        let repeated = stable_port_for(r"C:\apps\counter\target\telorgon-profile\counter.exe");
        assert_eq!(first, repeated);
        assert!((FIRST_STABLE_PORT..FIRST_STABLE_PORT + STABLE_PORT_COUNT).contains(&first));
    }

    #[test]
    fn every_managed_entrypoint_uses_the_same_session_contract() {
        for target in [ProfileTarget::Gui, ProfileTarget::DesktopEnvironment] {
            let config = ServerConfig::for_target(target);
            assert_eq!(config.metadata.entrypoint, target);
            assert!(
                config
                    .metadata
                    .capabilities
                    .iter()
                    .any(|value| value == "cpu-spans")
            );
            assert!(
                config
                    .metadata
                    .capabilities
                    .iter()
                    .any(|value| value == "frame-counters")
            );
        }
    }
}
