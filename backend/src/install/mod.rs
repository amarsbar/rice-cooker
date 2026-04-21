//! The install / uninstall / switch / list / status pipelines.
//!
//! Architecture is pre/post filesystem snapshot: diff two snapshots of the
//! watched roots (defaults + catalog extras), record what changed, reverse
//! it on uninstall. See `docs/install-design.md` (or the spec commit) for
//! the motivating design discussion.

pub mod diff;
pub mod env;
pub mod pacman;
pub mod pipeline;
pub mod record;
pub mod run;
pub mod snapshot;
pub mod systemd;

pub use env::{Dirs, resolve_dirs};
pub use pipeline::{Flags, install, list, status, switch, uninstall};
