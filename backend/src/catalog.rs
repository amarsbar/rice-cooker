//! Rice catalog — single `catalog.toml` file keyed by rice name.
//!
//! Hand-maintained in Rice Cooker's repo. Each entry describes the rice's
//! upstream, the pinned commit, the install command to run, and any
//! metadata the install/uninstall pipeline needs to treat paths correctly
//! (runtime-regenerated, partial-ownership, extra watched roots,
//! documented system effects that Rice Cooker cannot reverse).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

/// Top-level catalog: name → entry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Catalog {
    #[serde(flatten)]
    pub rices: BTreeMap<String, RiceEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RiceEntry {
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    pub repo: String,
    /// Full commit SHA — pinned for reproducibility. No "main" / "HEAD"
    /// aliases accepted; the catalog must declare a fixed point.
    pub commit: String,
    /// Command run in the clone dir as `/bin/sh -c "<install_cmd>"`. May
    /// reference any binary on PATH. For rices without a bespoke installer,
    /// synthesize something minimal like
    /// `mkdir -p "$HOME/.config/quickshell/<name>" && cp -rT . "$HOME/.config/quickshell/<name>"`.
    pub install_cmd: String,
    /// If true, pass the parent's tty through so install.sh can prompt.
    /// Otherwise stdin is /dev/null and stdout/stderr are captured to the
    /// log file (teed to the parent's stderr for visibility).
    #[serde(default)]
    pub interactive: bool,
    /// Determines how `try` launches this rice for preview.
    #[serde(default = "default_shell_type")]
    pub shell_type: ShellType,
    /// Paths the rice regenerates at runtime (theme engines, wallpaper
    /// hooks). On uninstall: restore pre-install content unconditionally,
    /// never `.rcsave` the current content.
    #[serde(default)]
    pub runtime_regenerated: Vec<String>,
    /// Paths with partial ownership — user may share the file with the
    /// rice. On uninstall: always `.rcsave` the current content, even if
    /// it matches the post-install hash.
    #[serde(default)]
    pub partial_ownership: Vec<String>,
    /// Extra watched roots beyond the defaults (~/.config, ~/.local/share,
    /// ~/.local/bin, ~/.local/lib). Must stay under `$HOME`.
    #[serde(default)]
    pub extra_watched_roots: Vec<String>,
    /// Purely informational: rendered in `list` / `status` so users know
    /// which effects Rice Cooker cannot reverse.
    #[serde(default)]
    pub documented_system_effects: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellType {
    Quickshell,
    Ags,
    Eww,
    Waybar,
    None,
}

fn default_shell_type() -> ShellType {
    ShellType::Quickshell
}

impl Catalog {
    // Intentionally NOT an impl of `FromStr` (infallible `Err = Infallible`
    // doesn't match our fallible story; `Catalog::from_str(body)` reads
    // better than `body.parse()` at call sites).
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

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.rices.keys().map(|s| s.as_str())
    }
}

/// Shape-check the catalog's pinned commit. All-zero hex is a placeholder
/// used during catalog bring-up; installs refuse with a clear message
/// instead of letting git surface `fatal: reference is not a tree`.
pub fn is_placeholder_commit(commit: &str) -> bool {
    commit.chars().all(|c| c == '0')
}

/// Rice names are path-segment-safe (they become cache/clone/log
/// directory names). Same rule as the v1 cache id validator.
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
    // Non-empty required fields.
    if entry.display_name.is_empty() {
        return Err(anyhow!("{name}: display_name is empty"));
    }
    if entry.repo.is_empty() {
        return Err(anyhow!("{name}: repo is empty"));
    }
    if entry.commit.is_empty() {
        return Err(anyhow!("{name}: commit is empty"));
    }
    // Commit must be a plausible hex SHA (≥7 hex chars). No branch names.
    // All-zero hex slips past parse (we let it) and is caught at install
    // time by `is_placeholder_commit` with a clearer, named error.
    if !entry.commit.chars().all(|c| c.is_ascii_hexdigit()) || entry.commit.len() < 7 {
        return Err(anyhow!(
            "{name}: commit must be a hex SHA (≥7 chars), got {:?}",
            entry.commit
        ));
    }
    if entry.install_cmd.is_empty() {
        return Err(anyhow!("{name}: install_cmd is empty"));
    }
    // Extra watched roots must look like absolute paths or start with ~
    // (expanded later). Refuse system paths explicitly.
    for root in &entry.extra_watched_roots {
        if !(root.starts_with('~') || root.starts_with('/')) {
            return Err(anyhow!(
                "{name}: extra_watched_root must start with ~ or /, got {root:?}"
            ));
        }
        for forbidden in ["/etc", "/usr", "/var", "/boot", "/opt", "/"] {
            if root == forbidden || root.starts_with(&format!("{forbidden}/")) {
                return Err(anyhow!(
                    "{name}: extra_watched_root cannot be under {forbidden}: {root:?}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        [noctalia]
        display_name = "Noctalia"
        repo = "https://github.com/noctalia-dev/noctalia-shell"
        commit = "0123456789abcdef0123456789abcdef01234567"
        install_cmd = "true"
    "#;

    #[test]
    fn parses_minimal() {
        let c = Catalog::from_str(MINIMAL).unwrap();
        assert_eq!(c.rices.len(), 1);
        let e = c.get("noctalia").unwrap();
        assert_eq!(e.display_name, "Noctalia");
        assert_eq!(e.shell_type, ShellType::Quickshell);
        assert!(e.runtime_regenerated.is_empty());
        assert!(!e.interactive);
    }

    #[test]
    fn rejects_empty_required_fields() {
        let t = r#"
            [x]
            display_name = ""
            repo = "https://x"
            commit = "abcdef1"
            install_cmd = "true"
        "#;
        assert!(Catalog::from_str(t).is_err());
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
                install_cmd = "true"
                "#
            );
            assert!(Catalog::from_str(&t).is_err(), "accepted bad commit {bad:?}");
        }
    }

    #[test]
    fn rejects_system_extra_watched_roots() {
        for bad in ["/etc/hypr", "/usr/share/x", "/", "/boot/bad"] {
            let t = format!(
                r#"
                [x]
                display_name = "X"
                repo = "https://x"
                commit = "0123456789abcdef0123456789abcdef01234567"
                install_cmd = "true"
                extra_watched_roots = ["{bad}"]
                "#
            );
            assert!(
                Catalog::from_str(&t).is_err(),
                "accepted system root {bad:?}"
            );
        }
    }

    #[test]
    fn accepts_full_entry() {
        let t = r#"
            [caelestia]
            display_name = "Caelestia"
            description = "the segsy rice"
            repo = "https://github.com/caelestia-dots/caelestia"
            commit = "a3f4b2c9e1d7000000000000000000000000abcd"
            install_cmd = "./install.fish --noconfirm --aur-helper=paru"
            interactive = false
            shell_type = "quickshell"
            runtime_regenerated = ["~/.config/gtk-3.0/colors.css"]
            partial_ownership = ["~/.zshrc"]
            extra_watched_roots = ["~/Pictures/wallpapers"]
            documented_system_effects = ["installs caelestia-meta from AUR"]
        "#;
        let c = Catalog::from_str(t).unwrap();
        let e = c.get("caelestia").unwrap();
        assert_eq!(e.partial_ownership, vec!["~/.zshrc"]);
        assert_eq!(e.runtime_regenerated.len(), 1);
        assert_eq!(e.documented_system_effects.len(), 1);
    }

    #[test]
    fn names_are_sorted_and_validated() {
        let t = r#"
            [zebra]
            display_name = "Z"
            repo = "https://z"
            commit = "0123456789abcdef0123456789abcdef01234567"
            install_cmd = "true"

            [alpha]
            display_name = "A"
            repo = "https://a"
            commit = "0123456789abcdef0123456789abcdef01234567"
            install_cmd = "true"
        "#;
        let c = Catalog::from_str(t).unwrap();
        let names: Vec<_> = c.names().collect();
        assert_eq!(names, vec!["alpha", "zebra"]);
    }
}
