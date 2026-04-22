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
    /// How this rice gets deployed. `Symlink` = one `ln -sfnT` from the
    /// user's config dir into the cached clone, no fs-diff tracking.
    /// `Dotfiles` = run `install_cmd` and diff pre/post snapshots of
    /// auto-scanned tracked paths. Default is `Dotfiles` for backwards
    /// compatibility with the pre-shape catalog schema.
    #[serde(default = "default_shape")]
    pub shape: Shape,
    /// Command run in the clone dir as `/bin/sh -c "<install_cmd>"`.
    /// Required for `Dotfiles`, forbidden for `Symlink`.
    #[serde(default)]
    pub install_cmd: String,
    /// For `Symlink` shape: path inside the clone to point at. Typically
    /// `"."` (repo root is the shell config). Relative to the clone dir.
    #[serde(default)]
    pub symlink_src: String,
    /// For `Symlink` shape: absolute `ln -sfnT` destination, typically
    /// under `~/.config/quickshell/<name>`. Tilde-expanded at install.
    #[serde(default)]
    pub symlink_dst: String,
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
    /// never `.rcsave` the current content. Dotfiles shape only.
    #[serde(default)]
    pub runtime_regenerated: Vec<String>,
    /// Paths with partial ownership — user may share the file with the
    /// rice. On uninstall: always `.rcsave` the current content, even if
    /// it matches the post-install hash. Dotfiles shape only.
    #[serde(default)]
    pub partial_ownership: Vec<String>,
    /// Extra watched roots beyond the defaults. Defaults live in
    /// `snapshot::DEFAULT_WATCHED_ROOTS` — `.config`, `.local/bin`,
    /// `.local/lib`, and the five `.local/share/{applications,fonts,
    /// icons,quickshell,themes}` dirs rices typically write into. A
    /// rice that deploys into other paths (e.g. `.local/share/matugen`)
    /// must declare them here. Must stay under `$HOME`. Dotfiles shape
    /// only.
    #[serde(default)]
    pub extra_watched_roots: Vec<String>,
    /// Repo packages installed before `install_cmd` runs (or before the
    /// symlink is created). Processed via the privileged helper binary
    /// (`pacman -S --needed --noconfirm <pkgs>`). Names must match
    /// `^[a-zA-Z0-9@._+-]+$`; the helper will reject anything else.
    #[serde(default)]
    pub pacman_deps: Vec<String>,
    /// AUR packages installed before `install_cmd` (or the symlink).
    /// Each entry must have a matching key in `aur_commits` pinning the
    /// PKGBUILD repo to a specific commit. Transitive AUR deps must be
    /// declared explicitly in dependency order — Rice Cooker does NOT
    /// walk the AUR dep graph.
    #[serde(default)]
    pub aur_deps: Vec<String>,
    /// Per-AUR-package commit pins. Every `aur_deps` entry must have a
    /// matching key here.
    #[serde(default)]
    pub aur_commits: std::collections::BTreeMap<String, String>,
    /// Purely informational: rendered in `list` / `status` so users know
    /// which effects Rice Cooker cannot reverse.
    #[serde(default)]
    pub documented_system_effects: Vec<String>,
}

/// Install shape — determines how the rice is deployed and tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    /// One `ln -sfnT <clone>/<symlink_src> <symlink_dst>`. No snapshot.
    /// Uninstall removes the symlink (after git-status-based rcsave of
    /// user edits inside the clone).
    Symlink,
    /// Run `install_cmd` and diff pre/post snapshots of watched roots.
    /// Uninstall reverses the fs-diff.
    Dotfiles,
}

fn default_shape() -> Shape {
    Shape::Dotfiles
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

/// Shape-check the catalog's pinned commit for placeholder patterns.
/// Real git SHAs have near-uniform hex distribution (first digit `0` is a
/// 1-in-16 event); a commit that begins with 30+ consecutive `0`s is
/// astronomically unlikely to be real and is almost certainly a
/// placeholder written during catalog bring-up. Install refuses with a
/// clear message instead of letting git surface the cryptic
/// `fatal: reference is not a tree: <commit>`.
///
/// The 30-zero threshold is lenient enough that a real SHA starting with
/// many zeros (rare but possible for someone grinding commits) still
/// passes so long as it has variety in the back half — at 30+ leading
/// zeros on a 40-char SHA, the attacker has ground 120 bits of work.
pub fn is_placeholder_commit(commit: &str) -> bool {
    commit.chars().take_while(|c| *c == '0').count() >= 30
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
    // Shape rules: symlink needs symlink_src+dst and forbids install_cmd;
    // dotfiles requires install_cmd and ignores symlink_* fields.
    match entry.shape {
        Shape::Symlink => {
            if !entry.install_cmd.is_empty() {
                return Err(anyhow!(
                    "{name}: install_cmd is forbidden for shape = \"symlink\" (use shape = \"dotfiles\" if the rice needs more than a symlink)"
                ));
            }
            // Dotfiles-only fields must not appear on Symlink entries.
            // Silently ignoring them would hide catalog mistakes where
            // the maintainer meant shape = "dotfiles" but typoed.
            if !entry.partial_ownership.is_empty() {
                return Err(anyhow!(
                    "{name}: partial_ownership is only valid for shape = \"dotfiles\""
                ));
            }
            if !entry.runtime_regenerated.is_empty() {
                return Err(anyhow!(
                    "{name}: runtime_regenerated is only valid for shape = \"dotfiles\""
                ));
            }
            if !entry.extra_watched_roots.is_empty() {
                return Err(anyhow!(
                    "{name}: extra_watched_roots is only valid for shape = \"dotfiles\""
                ));
            }
            if entry.symlink_src.is_empty() {
                return Err(anyhow!(
                    "{name}: symlink_src is required for shape = \"symlink\""
                ));
            }
            if entry.symlink_dst.is_empty() {
                return Err(anyhow!(
                    "{name}: symlink_dst is required for shape = \"symlink\""
                ));
            }
            if !(entry.symlink_dst.starts_with('~') || entry.symlink_dst.starts_with('/')) {
                return Err(anyhow!(
                    "{name}: symlink_dst must start with ~ or /, got {:?}",
                    entry.symlink_dst
                ));
            }
            // symlink_dst must be strictly under $HOME (expanded form), not
            // ~ itself, and not under /etc, /usr, etc.
            let dst = &entry.symlink_dst;
            if dst == "~" || dst == "~/" || dst == "/" {
                return Err(anyhow!(
                    "{name}: symlink_dst cannot be $HOME or / itself: {dst:?}"
                ));
            }
            for forbidden in ["/etc", "/usr", "/var", "/boot", "/opt"] {
                if dst == forbidden || dst.starts_with(&format!("{forbidden}/")) {
                    return Err(anyhow!(
                        "{name}: symlink_dst cannot be under {forbidden}: {dst:?}"
                    ));
                }
            }
            if dst.contains("..") {
                return Err(anyhow!(
                    "{name}: symlink_dst must not contain .. segments: {dst:?}"
                ));
            }
        }
        Shape::Dotfiles => {
            if entry.install_cmd.is_empty() {
                return Err(anyhow!(
                    "{name}: install_cmd is required for shape = \"dotfiles\""
                ));
            }
            if !entry.symlink_src.is_empty() || !entry.symlink_dst.is_empty() {
                return Err(anyhow!(
                    "{name}: symlink_src/symlink_dst are only valid for shape = \"symlink\""
                ));
            }
        }
    }
    // Every aur_deps entry must have a matching aur_commits pin.
    for pkg in &entry.aur_deps {
        if !entry.aur_commits.contains_key(pkg) {
            return Err(anyhow!(
                "{name}: aur_deps contains {pkg:?} but aur_commits has no pin for it"
            ));
        }
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
        // Default shape is Dotfiles (backwards-compat with pre-shape catalogs).
        assert_eq!(e.shape, Shape::Dotfiles);
        assert!(e.runtime_regenerated.is_empty());
        assert!(!e.interactive);
    }

    #[test]
    fn parses_symlink_shape() {
        let t = r#"
            [dms]
            display_name = "DMS"
            repo = "https://github.com/x/y"
            commit = "0123456789abcdef0123456789abcdef01234567"
            shape = "symlink"
            symlink_src = "."
            symlink_dst = "~/.config/quickshell/dms"
        "#;
        let c = Catalog::from_str(t).unwrap();
        let e = c.get("dms").unwrap();
        assert_eq!(e.shape, Shape::Symlink);
        assert_eq!(e.symlink_src, ".");
        assert_eq!(e.symlink_dst, "~/.config/quickshell/dms");
        assert!(e.install_cmd.is_empty());
    }

    #[test]
    fn symlink_shape_rejects_install_cmd() {
        let t = r#"
            [x]
            display_name = "X"
            repo = "https://x"
            commit = "0123456789abcdef0123456789abcdef01234567"
            shape = "symlink"
            symlink_src = "."
            symlink_dst = "~/.config/x"
            install_cmd = "true"
        "#;
        assert!(Catalog::from_str(t).is_err());
    }

    #[test]
    fn symlink_shape_requires_symlink_fields() {
        let t = r#"
            [x]
            display_name = "X"
            repo = "https://x"
            commit = "0123456789abcdef0123456789abcdef01234567"
            shape = "symlink"
        "#;
        assert!(Catalog::from_str(t).is_err());
    }

    #[test]
    fn symlink_shape_rejects_system_symlink_dst() {
        for bad in ["/etc/x", "/usr/share/x", "/", "~", "~/../escape"] {
            let t = format!(
                r#"
                [x]
                display_name = "X"
                repo = "https://x"
                commit = "0123456789abcdef0123456789abcdef01234567"
                shape = "symlink"
                symlink_src = "."
                symlink_dst = "{bad}"
                "#
            );
            assert!(
                Catalog::from_str(&t).is_err(),
                "accepted unsafe symlink_dst {bad:?}"
            );
        }
    }

    #[test]
    fn symlink_shape_rejects_dotfiles_only_fields() {
        for (field, value) in [
            ("partial_ownership", "[\"~/.zshrc\"]"),
            ("runtime_regenerated", "[\"~/.config/gtk-3.0/colors.css\"]"),
            ("extra_watched_roots", "[\"~/.local/share/matugen\"]"),
        ] {
            let t = format!(
                r#"
                [x]
                display_name = "X"
                repo = "https://x"
                commit = "0123456789abcdef0123456789abcdef01234567"
                shape = "symlink"
                symlink_src = "."
                symlink_dst = "~/.config/quickshell/x"
                {field} = {value}
                "#
            );
            let err = Catalog::from_str(&t).unwrap_err().to_string();
            assert!(
                err.contains(field),
                "expected {field:?} in error, got: {err}"
            );
        }
    }

    #[test]
    fn dotfiles_shape_forbids_symlink_fields() {
        let t = r#"
            [x]
            display_name = "X"
            repo = "https://x"
            commit = "0123456789abcdef0123456789abcdef01234567"
            shape = "dotfiles"
            install_cmd = "true"
            symlink_src = "."
        "#;
        assert!(Catalog::from_str(t).is_err());
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
            assert!(
                Catalog::from_str(&t).is_err(),
                "accepted bad commit {bad:?}"
            );
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
