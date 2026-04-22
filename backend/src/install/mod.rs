//! install / uninstall / switch / list / status commands.

pub mod env;
pub mod pipeline;
pub mod record;
pub mod symlink;

pub use env::{Dirs, resolve_dirs};
pub use pipeline::{
    Flags, InstallOutcome, ListRow, StatusRow, SwitchOutcome, UninstallOutcome, install, list,
    status, switch, uninstall,
};
