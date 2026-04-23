//! Rice catalog — single `catalog.toml` file keyed by rice name.
//!
//! Hand-maintained in Rice Cooker's repo. Each entry: upstream repo at
//! a pinned commit, a symlink src/dst that points into the clone, and
//! optional `pacman_deps` / `aur_deps` (paru resolves transitive AUR
//! deps — we only list top-level names).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(flatten)]
    pub rices: BTreeMap<String, RiceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RiceEntry {
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub repo: String,
    /// Full commit SHA — pinned for reproducibility. Placeholder values
    /// containing "PLACEHOLDER" are refused at install time.
    pub commit: String,
    /// Install is one `ln -sfnT <clone>/<symlink_src> <symlink_dst>`.
    pub symlink_src: String,
    /// Absolute `ln -sfnT` destination, `~`-expanded. Must stay under
    /// `$HOME`.
    pub symlink_dst: String,
    /// Top-level AUR dep names. Paru resolves transitive deps. No
    /// commit pins — paru builds whatever the AUR maintainer published.
    #[serde(default)]
    pub aur_deps: Vec<String>,
    /// Repo-only packages the rice needs that paru wouldn't pick up
    /// from aur_deps' own depends lists. Usually empty.
    #[serde(default)]
    pub pacman_deps: Vec<String>,
    /// Reserved for future interactive-installer support. Set to true ⇒
    /// install refuses (see `docs/issues/interactive-installs.md`).
    #[serde(default)]
    pub interactive: bool,
    /// Purely informational — shown in `list` / `status` for effects
    /// outside Rice Cooker's control (system services, root-owned
    /// configs, etc.).
    #[serde(default)]
    pub documented_system_effects: Vec<String>,
}

impl Catalog {
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Result<Self> {
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
        Self::from_str(&body)
    }

    pub fn get(&self, name: &str) -> Option<&RiceEntry> {
        self.rices.get(name)
    }
}

/// Install-time refusal: commit contains "PLACEHOLDER" (uncurated
/// catalog entry). Catches catalog bring-up errors before paru does
/// network work.
pub fn is_placeholder_commit(commit: &str) -> bool {
    commit.contains("PLACEHOLDER")
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.starts_with('-')
        || name.chars().any(|c| matches!(c, '/' | '\\' | '\0'))
    {
        return Err(anyhow!("invalid rice name {name:?}"));
    }
    Ok(())
}

fn validate_entry(name: &str, entry: &RiceEntry) -> Result<()> {
    if entry.display_name.is_empty() {
        return Err(anyhow!("{name}: display_name is empty"));
    }
    if entry.repo.is_empty() {
        return Err(anyhow!("{name}: repo is empty"));
    }
    if entry.commit.is_empty() {
        return Err(anyhow!("{name}: commit is empty"));
    }
    // Commit must be plausible hex SHA OR explicit placeholder text.
    // Placeholder ("PLACEHOLDER..." etc.) parses through so `list`/
    // `status` can inspect unreleased entries; `install` refuses at
    // runtime via is_placeholder_commit.
    let is_hex = entry.commit.chars().all(|c| c.is_ascii_hexdigit());
    let is_placeholder = entry.commit.contains("PLACEHOLDER");
    if !(is_hex || is_placeholder) || (is_hex && entry.commit.len() < 7) {
        return Err(anyhow!(
            "{name}: commit must be a hex SHA (≥7 chars) or contain \"PLACEHOLDER\", got {:?}",
            entry.commit
        ));
    }
    if entry.interactive {
        return Err(anyhow!(
            "{name}: interactive = true is not supported in v1 (see docs/issues/interactive-installs.md)"
        ));
    }
    if entry.symlink_src.is_empty() {
        return Err(anyhow!("{name}: symlink_src is required"));
    }
    // symlink_src is joined onto clone_dir at install time. Reject
    // absolute paths (which Path::join would treat as a full replacement,
    // escaping clone_dir entirely) and any `..` component (Path::join
    // preserves `..` literally; the OS backs out of clone_dir when it
    // later dereferences the joined path).
    let src = std::path::Path::new(&entry.symlink_src);
    if src.is_absolute() {
        return Err(anyhow!(
            "{name}: symlink_src must be relative to the clone dir, got {:?}",
            entry.symlink_src
        ));
    }
    if src
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(anyhow!(
            "{name}: symlink_src must not contain .. segments, got {:?}",
            entry.symlink_src
        ));
    }
    if entry.symlink_dst.is_empty() {
        return Err(anyhow!("{name}: symlink_dst is required"));
    }
    let dst = &entry.symlink_dst;
    // Doc contract: dst must stay under `$HOME`. Enforce structurally by
    // requiring the `~/<subpath>` shape. Bare absolute paths are out —
    // `/etc/x`, `/usr/x`, `/home/other-user/x`, etc. all fail here
    // instead of leaking through a "forbidden-prefix" blocklist that
    // would miss `/home/other-user`, `/tmp`, `/mnt`, and any path the
    // list didn't happen to enumerate.
    if !dst.starts_with("~/") {
        return Err(anyhow!(
            "{name}: symlink_dst must be under $HOME (start with `~/`), got {dst:?}"
        ));
    }
    if dst == "~/" {
        return Err(anyhow!(
            "{name}: symlink_dst cannot be $HOME itself: {dst:?}"
        ));
    }
    // Use Path::components for the `..` check (mirrors the symlink_src
    // check). `dst.contains("..")` would both over-match (rejecting
    // benign names like `..foo`) and confuse real traversal with
    // substring noise; `Component::ParentDir` matches only a pure `..`
    // component, which is exactly what escapes `$HOME`.
    let dst_path = std::path::Path::new(dst);
    if dst_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(anyhow!(
            "{name}: symlink_dst must not contain .. components: {dst:?}"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        [dms]
        display_name = "DMS"
        repo = "https://x/dms"
        commit = "0123456789abcdef0123456789abcdef01234567"
        symlink_src = "."
        symlink_dst = "~/.config/quickshell/dms"
    "#;

    #[test]
    fn parses_minimal() {
        let c = Catalog::from_str(MINIMAL).unwrap();
        let e = c.get("dms").unwrap();
        assert_eq!(e.display_name, "DMS");
        assert!(e.aur_deps.is_empty());
        assert!(e.pacman_deps.is_empty());
        assert!(!e.interactive);
    }

    #[test]
    fn rejects_interactive_true() {
        let t = r#"
            [x]
            display_name = "X"
            repo = "https://x"
            commit = "0123456789abcdef0123456789abcdef01234567"
            symlink_src = "."
            symlink_dst = "~/.config/quickshell/x"
            interactive = true
        "#;
        let err = Catalog::from_str(t).unwrap_err().to_string();
        assert!(err.contains("interactive"), "got: {err}");
    }

    #[test]
    fn accepts_placeholder_commit_at_parse() {
        // Parser accepts placeholder; install refuses via
        // is_placeholder_commit. list/status still works.
        let t = r#"
            [x]
            display_name = "X"
            repo = "https://x"
            commit = "PLACEHOLDER0123"
            symlink_src = "."
            symlink_dst = "~/.config/quickshell/x"
        "#;
        assert!(Catalog::from_str(t).is_ok());
    }

    #[test]
    fn is_placeholder_detects_placeholder_text() {
        assert!(is_placeholder_commit("PLACEHOLDER..."));
        assert!(is_placeholder_commit("abcPLACEHOLDERdef"));
        assert!(!is_placeholder_commit(
            "0123456789abcdef0123456789abcdef01234567"
        ));
    }

    #[test]
    fn rejects_non_sha_commit() {
        for bad in ["main", "HEAD", "v1.0", "abc"] {
            let t = format!(
                r#"
                [x]
                display_name = "X"
                repo = "https://x"
                commit = "{bad}"
                symlink_src = "."
                symlink_dst = "~/.config/quickshell/x"
                "#
            );
            assert!(Catalog::from_str(&t).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn rejects_symlink_dst_outside_home() {
        for bad in ["/etc/x", "/usr/share/x", "/", "~", "~/../escape"] {
            let t = format!(
                r#"
                [x]
                display_name = "X"
                repo = "https://x"
                commit = "0123456789abcdef0123456789abcdef01234567"
                symlink_src = "."
                symlink_dst = "{bad}"
                "#
            );
            assert!(Catalog::from_str(&t).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn refuses_missing_required_fields() {
        for body in [
            r#"[x]
               display_name = ""
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               repo = ""
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = ""
               symlink_dst = "~/.config/x""#,
            r#"[x]
               display_name = "X"
               repo = "https://x"
               commit = "0123456789abcdef0123456789abcdef01234567"
               symlink_src = "."
               symlink_dst = """#,
        ] {
            assert!(Catalog::from_str(body).is_err(), "accepted: {body}");
        }
    }

    #[test]
    fn round_trips_with_deps() {
        let t = r#"
            [caelestia]
            display_name = "Caelestia"
            repo = "https://github.com/caelestia-dots/caelestia"
            commit = "0283b44960791ab12cde19c9797d70976a0b96a4"
            symlink_src = "."
            symlink_dst = "~/.config/quickshell/caelestia"
            aur_deps = ["caelestia-shell-git"]
        "#;
        let c = Catalog::from_str(t).unwrap();
        let e = c.get("caelestia").unwrap();
        assert_eq!(e.aur_deps, vec!["caelestia-shell-git"]);
    }
}
