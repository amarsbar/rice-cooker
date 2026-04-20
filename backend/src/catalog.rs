//! Rice catalog: parses per-rice `rice.toml` entries shipped in-tree.
//!
//! Each catalog entry describes a curated Quickshell rice — where to clone
//! from, which file is the entry point, optional pacman dep hints, and
//! optional per-file deployment overrides. If no `[[files]]` table is
//! present, the install pipeline falls back to the default rule: symlink
//! the parent directory of `rice.entry` into
//! `$XDG_CONFIG_HOME/quickshell/<id>/`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

/// A parsed catalog entry.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Catalog {
    pub rice: RiceMeta,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Deps>,
    /// Per-file deployment overrides. Empty = use the default rule.
    #[serde(default, rename = "files", skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileSpec>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct RiceMeta {
    pub id: String,
    pub name: String,
    pub upstream: String,
    /// Path to the rice's `shell.qml` relative to the repo root. Defaults to
    /// `"shell.qml"`. Mirrors the `--entry` flag on `apply`.
    #[serde(default = "default_entry")]
    pub entry: String,
}

fn default_entry() -> String {
    "shell.qml".to_string()
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Deps {
    #[serde(default)]
    pub pacman: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Symlink,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct FileSpec {
    /// Path relative to the rice's repo root.
    pub src: String,
    /// Destination path. Supports `$XDG_CONFIG_HOME` and `$HOME` placeholders,
    /// resolved at deploy time against the running process's environment.
    pub dest: String,
    pub mode: Mode,
}

impl Catalog {
    /// Parse a catalog from TOML bytes. Rejects entries whose `rice.id`
    /// contains path-dangerous characters — the id is used both as a
    /// filesystem path segment (cache dir, quickshell target dir) and as a
    /// catalog filename stem, so the guarantee must match `Cache::rice_dir`.
    pub fn from_str(s: &str) -> Result<Self> {
        let cat: Catalog = toml::from_str(s).context("parsing rice catalog TOML")?;
        validate_id(&cat.rice.id)?;
        Ok(cat)
    }

    pub fn from_file(path: &Path) -> Result<Self> {
        let body = fs::read_to_string(path)
            .with_context(|| format!("reading catalog {}", path.display()))?;
        Self::from_str(&body)
    }

    /// The default deployment target directory: `$XDG_CONFIG_HOME/quickshell/<id>/`.
    /// Used when `files` is empty.
    pub fn default_deploy_dest(&self, xdg_config_home: &Path) -> PathBuf {
        xdg_config_home.join("quickshell").join(&self.rice.id)
    }

    /// The rice-repo-relative source directory for the default deployment rule:
    /// the parent of `entry`. For `entry = "shell.qml"` this is the repo root
    /// (`""`); for `entry = "quickshell/shell.qml"` it's `"quickshell"`.
    pub fn default_deploy_src(&self) -> PathBuf {
        Path::new(&self.rice.entry)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default()
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id == "."
        || id == ".."
        || id.chars().any(|c| matches!(c, '/' | '\\' | '\0'))
    {
        return Err(anyhow!("invalid rice id {id:?}"));
    }
    Ok(())
}

/// Expand `$XDG_CONFIG_HOME` and `$HOME` inside a catalog `dest` string using
/// the supplied environment values. Unset/empty `XDG_CONFIG_HOME` falls back
/// to `$HOME/.config` per XDG spec. Other env vars are *not* expanded — we
/// only want the two the schema documents, so typos like `$XGD_CONFIG_HOME`
/// surface as an error rather than silently resolving to an empty string.
pub fn expand_dest(dest: &str, home: Option<&str>, xdg_config_home: Option<&str>) -> Result<PathBuf> {
    let home = home.filter(|s| !s.is_empty());
    let xdg = xdg_config_home.filter(|s| !s.is_empty());
    let mut out = dest.to_string();
    // Longer token first so `$XDG_CONFIG_HOME` doesn't partial-match `$HOME`.
    if out.contains("$XDG_CONFIG_HOME") {
        let replacement = match (xdg, home) {
            (Some(x), _) => x.to_string(),
            (None, Some(h)) => format!("{h}/.config"),
            (None, None) => {
                return Err(anyhow!(
                    "dest uses $XDG_CONFIG_HOME but neither XDG_CONFIG_HOME nor HOME is set"
                ));
            }
        };
        out = out.replace("$XDG_CONFIG_HOME", &replacement);
    }
    if out.contains("$HOME") {
        let h = home.ok_or_else(|| anyhow!("dest uses $HOME but HOME is not set"))?;
        out = out.replace("$HOME", h);
    }
    if out.contains('$') {
        return Err(anyhow!(
            "dest contains an unrecognized placeholder (only $HOME and $XDG_CONFIG_HOME supported): {dest:?}"
        ));
    }
    Ok(PathBuf::from(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_entry_with_default_entry_path() {
        let toml = r#"
            [rice]
            id = "noctalia"
            name = "Noctalia"
            upstream = "https://github.com/noctalia-dev/noctalia-shell"
        "#;
        let cat = Catalog::from_str(toml).unwrap();
        assert_eq!(cat.rice.id, "noctalia");
        assert_eq!(cat.rice.name, "Noctalia");
        assert_eq!(cat.rice.upstream, "https://github.com/noctalia-dev/noctalia-shell");
        assert_eq!(cat.rice.entry, "shell.qml");
        assert!(cat.dependencies.is_none());
        assert!(cat.files.is_empty());
    }

    #[test]
    fn parses_full_entry_with_deps_and_files() {
        let toml = r#"
            [rice]
            id = "caelestia"
            name = "Caelestia"
            upstream = "https://github.com/caelestia-dots/shell"
            entry = "quickshell/shell.qml"

            [dependencies]
            pacman = ["quickshell", "qt6-5compat"]

            [[files]]
            src = "quickshell"
            dest = "$XDG_CONFIG_HOME/quickshell/caelestia"
            mode = "symlink"

            [[files]]
            src = "btop/btop.conf"
            dest = "$XDG_CONFIG_HOME/btop/btop.conf"
            mode = "copy"
        "#;
        let cat = Catalog::from_str(toml).unwrap();
        assert_eq!(cat.rice.entry, "quickshell/shell.qml");
        let deps = cat.dependencies.unwrap();
        assert_eq!(deps.pacman, vec!["quickshell", "qt6-5compat"]);
        assert_eq!(cat.files.len(), 2);
        assert_eq!(cat.files[0].mode, Mode::Symlink);
        assert_eq!(cat.files[1].mode, Mode::Copy);
    }

    #[test]
    fn rejects_missing_required_rice_fields() {
        // Missing `name` + `upstream` — both required (no default).
        let toml = r#"
            [rice]
            id = "x"
        "#;
        assert!(Catalog::from_str(toml).is_err());
    }

    #[test]
    fn rejects_id_with_path_traversal_or_separator() {
        for bad in ["", ".", "..", "a/b", r"a\b", "a\0b"] {
            let toml = format!(
                r#"
                [rice]
                id = "{}"
                name = "X"
                upstream = "https://example.com/x"
                "#,
                bad.replace('\0', r"\0").replace('\\', r"\\"),
            );
            let result = Catalog::from_str(&toml);
            // Either TOML parse fails (e.g. for backslash / nul escapes) or our
            // validator rejects. Either outcome is acceptable — we just don't
            // want any of these ids to round-trip.
            assert!(result.is_err(), "accepted bad id {bad:?}");
        }
    }

    #[test]
    fn default_deploy_src_is_entry_parent() {
        let mut cat = Catalog {
            rice: RiceMeta {
                id: "x".into(),
                name: "X".into(),
                upstream: "u".into(),
                entry: "shell.qml".into(),
            },
            dependencies: None,
            files: Vec::new(),
        };
        assert_eq!(cat.default_deploy_src(), PathBuf::new());
        cat.rice.entry = "quickshell/shell.qml".into();
        assert_eq!(cat.default_deploy_src(), PathBuf::from("quickshell"));
        cat.rice.entry = ".config/quickshell/shell.qml".into();
        assert_eq!(
            cat.default_deploy_src(),
            PathBuf::from(".config/quickshell")
        );
    }

    #[test]
    fn default_deploy_dest_is_xdg_config_quickshell_id() {
        let cat = Catalog {
            rice: RiceMeta {
                id: "caelestia".into(),
                name: "Caelestia".into(),
                upstream: "u".into(),
                entry: "shell.qml".into(),
            },
            dependencies: None,
            files: Vec::new(),
        };
        let xdg = Path::new("/home/x/.config");
        assert_eq!(
            cat.default_deploy_dest(xdg),
            PathBuf::from("/home/x/.config/quickshell/caelestia")
        );
    }

    #[test]
    fn expand_dest_resolves_xdg_and_home() {
        assert_eq!(
            expand_dest("$XDG_CONFIG_HOME/quickshell/x", Some("/h"), Some("/c")).unwrap(),
            PathBuf::from("/c/quickshell/x")
        );
        // XDG unset → falls back to HOME/.config.
        assert_eq!(
            expand_dest("$XDG_CONFIG_HOME/quickshell/x", Some("/h"), None).unwrap(),
            PathBuf::from("/h/.config/quickshell/x")
        );
        // Empty string is treated like unset.
        assert_eq!(
            expand_dest("$XDG_CONFIG_HOME/x", Some("/h"), Some("")).unwrap(),
            PathBuf::from("/h/.config/x")
        );
        // $HOME expansion.
        assert_eq!(
            expand_dest("$HOME/.config/x", Some("/h"), None).unwrap(),
            PathBuf::from("/h/.config/x")
        );
        // Both unset + uses $XDG_CONFIG_HOME → error.
        assert!(expand_dest("$XDG_CONFIG_HOME/x", None, None).is_err());
        // Uses $HOME without HOME set → error.
        assert!(expand_dest("$HOME/x", None, Some("/c")).is_err());
        // Typo'd placeholder → error (avoids silent empty-string expansion).
        assert!(expand_dest("$XGD_CONFIG_HOME/x", Some("/h"), Some("/c")).is_err());
    }
}
