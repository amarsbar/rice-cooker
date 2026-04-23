//! install / uninstall / switch / list / status commands.

pub mod pipeline;
pub mod record;
pub mod symlink;

pub use pipeline::{
    Flags, InstallOutcome, ListRow, StatusRow, SwitchOutcome, UninstallOutcome, install, list,
    status, switch, uninstall,
};
