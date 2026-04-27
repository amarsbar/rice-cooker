//! install pipeline + records + symlink shaping.

pub mod pipeline;
pub mod record;
pub mod symlink;

pub use pipeline::{Flags, ListRow, StatusRow, list, run_preview, run_try, run_uninstall, status};
