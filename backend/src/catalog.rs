//! Rice catalog — single `catalog.toml` file keyed by rice name.
//!
//! Hand-maintained in Rice Cooker's repo. Each entry: upstream repo at
//! a pinned commit, a symlink src/dst that points into the clone, and
//! optional dependency lists (yay/paru resolve repo vs AUR packages and
//! transitive deps — we only list top-level names).

use std::fs;
use std::path::{Component, Path};

use anyhow::{Context, Result, ensure};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(flatten)]
    pub rices: IndexMap<String, RiceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiceEntry {
    pub display_name: String,
    pub creator_name: String,
    pub repo: String,
    /// Ref passed to `git checkout --detach`.
    pub commit: String,
    /// Source path installed by the symlink step, relative to the clone dir.
    pub symlink_src: String,
    /// Symlink destination, `~`-expanded. Must stay under `$HOME`.
    pub symlink_dst: String,
    /// True when the package installs a Quickshell config discoverable by `-c`.
    #[serde(default)]
    pub package_managed: bool,
    /// Minimal deps needed even for dependency-light preview.
    #[serde(default)]
    pub preview_deps: Vec<String>,
    /// Full install deps. Empty means install is unavailable.
    #[serde(default)]
    pub install_deps: Vec<String>,
    /// Reserved for future interactive-installer support. Set to true ⇒
    /// install refuses (see `docs/issues/interactive-installs.md`).
    #[serde(default)]
    pub interactive: bool,
}

impl Catalog {
    pub fn parse(s: &str) -> Result<Self> {
        let cat: Catalog = toml::from_str(s).context("parsing catalog.toml")?;
        for (name, entry) in &cat.rices {
            validate_name(name)?;
            validate_entry(name, entry)?;
        }
        Ok(cat)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let body = fs::read_to_string(path)
            .with_context(|| format!("reading catalog {}", path.display()))?;
        Self::parse(&body)
    }

    pub fn get(&self, name: &str) -> Option<&RiceEntry> {
        self.rices.get(name)
    }
}

pub fn validate_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('-')
        || name.chars().any(|c| matches!(c, '/' | '\\' | '\0'));
    ensure!(!bad, "invalid rice name {name:?}");
    Ok(())
}

fn validate_entry(name: &str, entry: &RiceEntry) -> Result<()> {
    ensure!(
        !entry.display_name.is_empty(),
        "{name}: display_name is empty"
    );
    ensure!(
        !entry.creator_name.is_empty(),
        "{name}: creator_name is empty"
    );
    ensure!(!entry.repo.is_empty(), "{name}: repo is empty");
    ensure!(!entry.commit.is_empty(), "{name}: commit is empty");

    ensure!(
        !entry.interactive,
        "{name}: interactive = true is not supported in v1 (see docs/issues/interactive-installs.md)"
    );

    ensure!(
        !entry.symlink_src.is_empty(),
        "{name}: symlink_src is required"
    );
    // symlink_src gets Path::join'd onto clone_dir; absolute paths would
    // escape clone_dir outright and `..` would escape at dereference time.
    let src = Path::new(&entry.symlink_src);
    ensure!(
        !src.is_absolute(),
        "{name}: symlink_src must be relative to the clone dir, got {:?}",
        entry.symlink_src
    );
    ensure!(
        !src.components().any(|c| matches!(c, Component::ParentDir)),
        "{name}: symlink_src must not contain .. segments, got {:?}",
        entry.symlink_src
    );

    let dst = &entry.symlink_dst;
    ensure!(!dst.is_empty(), "{name}: symlink_dst is required");
    ensure!(
        dst.starts_with("~/"),
        "{name}: symlink_dst must be under $HOME (start with `~/`), got {dst:?}"
    );
    ensure!(
        dst != "~/",
        "{name}: symlink_dst cannot be $HOME itself: {dst:?}"
    );
    ensure!(
        !Path::new(dst)
            .components()
            .any(|c| matches!(c, Component::ParentDir)),
        "{name}: symlink_dst must not contain .. components: {dst:?}"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        [dms]
        display_name = "DMS"
        creator_name = "AvengeMedia"
        repo = "https://x/dms"
        commit = "0123456789abcdef0123456789abcdef01234567"
        symlink_src = "."
        symlink_dst = "~/.config/quickshell/dms"
    "#;

    #[test]
    fn parses_minimal() {
        let c = Catalog::parse(MINIMAL).unwrap();
        let e = c.get("dms").unwrap();
        assert_eq!(e.display_name, "DMS");
        assert_eq!(e.creator_name, "AvengeMedia");
        assert!(e.preview_deps.is_empty());
        assert!(e.install_deps.is_empty());
        assert!(!e.package_managed);
        assert!(!e.interactive);
    }

    #[test]
    fn rejects_interactive_true() {
        let t = r#"
            [x]
            display_name = "X"
            creator_name = "x"
            repo = "https://x"
            commit = "0123456789abcdef0123456789abcdef01234567"
            symlink_src = "."
            symlink_dst = "~/.config/quickshell/x"
            interactive = true
        "#;
        let err = Catalog::parse(t).unwrap_err().to_string();
        assert!(err.contains("interactive"), "got: {err}");
    }

    #[test]
    fn rejects_symlink_dst_outside_home() {
        for bad in ["/etc/x", "/usr/share/x", "/", "~", "~/../escape"] {
            let t = format!(
                r#"
                [x]
                display_name = "X"
                creator_name = "x"
                repo = "https://x"
                commit = "0123456789abcdef0123456789abcdef01234567"
                symlink_src = "."
                symlink_dst = "{bad}"
                "#
            );
            assert!(Catalog::parse(&t).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn refuses_missing_required_fields() {
        for body in [
            r#"[x]
               display_name = ""
               creator_name = "x"
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               creator_name = ""
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               creator_name = "x"
               repo = ""
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               creator_name = "x"
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = ""
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               creator_name = "x"
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = """#,
        ] {
            assert!(Catalog::parse(body).is_err(), "accepted: {body}");
        }
    }

    #[test]
    fn round_trips_with_deps() {
        let t = r#"
            [caelestia]
            display_name = "Caelestia"
            creator_name = "soramenew"
            repo = "https://github.com/caelestia-dots/shell"
            commit = "efc08759ceaeddc2c571d868c623995270ac365d"
            symlink_src = "."
            symlink_dst = "~/.config/quickshell/caelestia"
            package_managed = true
            install_deps = ["quickshell-git", "caelestia-shell"]
            preview_deps = ["quickshell-git", "caelestia-shell"]
        "#;
        let c = Catalog::parse(t).unwrap();
        let e = c.get("caelestia").unwrap();
        assert!(e.package_managed);
        assert_eq!(e.install_deps, vec!["quickshell-git", "caelestia-shell"]);
        assert_eq!(e.preview_deps, vec!["quickshell-git", "caelestia-shell"]);
    }
}
