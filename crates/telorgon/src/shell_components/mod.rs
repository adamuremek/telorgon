//! Accessible, protocol-neutral shell components over immutable host snapshots.
//!
//! This crate mounts presentation and emits typed intentions. Native protocol objects, policy,
//! host mutation, rendering resources, event loops, tasks, and threads remain outside it.

pub mod chrome;
pub mod diagnostics;
pub mod launcher;
pub mod notification;
pub mod panel;
pub mod prelude;
pub mod secure;
pub mod status;
pub mod theme;
pub mod workspace;

pub use chrome::*;
pub use diagnostics::*;
pub use launcher::*;
pub use notification::*;
pub use panel::*;
pub use secure::*;
pub use status::*;
pub use theme::*;
pub use workspace::*;
