//! install pipeline + records + symlink shaping.

pub mod pipeline;
pub mod record;
pub mod symlink;

pub use pipeline::{
    Flags, ListRow, StatusRow, list, run_install, run_preview, run_uninstall, status,
};
