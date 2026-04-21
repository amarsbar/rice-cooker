//! Systemd user-unit detection + uninstall disable.
//!
//! We look at the install's `fs_diff.symlinks_added` for entries under
//! `$HOME/.config/systemd/user/**/*.target.wants/*.service`. That's how
//! `systemctl --user enable` creates enablements — as symlinks into the
//! unit's canonical location. Name = the symlink's filename (strip `.service`
//! for logging; keep the full filename for `systemctl --user disable`).

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::Result;

use super::diff::FsDiff;

/// Extract unit filenames (e.g. `caelestia-shell.service`) from the
/// `symlinks_added` entries that live under a `.target.wants/` directory
/// under `$HOME/.config/systemd/user/`.
pub fn detect_enabled_units(home: &Path, diff: &FsDiff) -> Vec<String> {
    let wants_root = home.join(".config/systemd/user");
    let mut out: Vec<String> = Vec::new();
    for link in &diff.symlinks_added {
        // Must live under ~/.config/systemd/user/
        if !link.path.starts_with(&wants_root) {
            continue;
        }
        // Parent must be `<something>.target.wants/`.
        let Some(parent) = link.path.parent() else {
            continue;
        };
        let Some(parent_name) = parent.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !parent_name.ends_with(".target.wants") {
            continue;
        }
        // Filename must be *.service (covers .socket, .timer too — we
        // accept any unit extension by being permissive).
        let Some(name) = link.path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        out.push(name.to_string());
    }
    out.sort();
    out.dedup();
    out
}

/// Disable + stop each unit via `systemctl --user disable --now`. Failures
/// are logged to stderr but non-fatal — if the user already disabled a unit
/// manually, systemctl exits non-zero with "does not exist"; we log and
/// continue so uninstall can finish cleanly.
pub fn disable_units(units: &[String]) -> Result<()> {
    for unit in units {
        let status = Command::new("systemctl")
            .args(["--user", "disable", "--now", unit])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => {
                eprintln!(
                    "systemctl --user disable --now {unit} exited {:?}; continuing",
                    s.code()
                );
            }
            Err(e) => {
                // systemctl not installed or no user session — just log.
                eprintln!("failed to spawn systemctl for {unit}: {e}; continuing");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::diff::AddedSymlink;
    use std::path::PathBuf;

    fn link(path: &str, target: &str) -> AddedSymlink {
        AddedSymlink {
            path: PathBuf::from(path),
            target: PathBuf::from(target),
            mode: 0o777,
        }
    }

    #[test]
    fn detects_user_units_in_target_wants() {
        let mut diff = FsDiff::default();
        diff.symlinks_added.push(link(
            "/h/.config/systemd/user/graphical-session.target.wants/caelestia-shell.service",
            "/h/.config/systemd/user/caelestia-shell.service",
        ));
        diff.symlinks_added.push(link(
            "/h/.config/systemd/user/default.target.wants/matugen.timer",
            "/h/.config/systemd/user/matugen.timer",
        ));
        // Not in target.wants — should be ignored.
        diff.symlinks_added.push(link(
            "/h/.config/systemd/user/caelestia-shell.service",
            "/elsewhere",
        ));
        // Outside ~/.config/systemd/user — ignored.
        diff.symlinks_added.push(link(
            "/etc/systemd/system/graphical.target.wants/foo.service",
            "/foo",
        ));
        let got = detect_enabled_units(Path::new("/h"), &diff);
        assert_eq!(got, vec!["caelestia-shell.service", "matugen.timer"]);
    }

    #[test]
    fn ignores_symlinks_not_under_target_wants() {
        let mut diff = FsDiff::default();
        diff.symlinks_added.push(link(
            "/h/.config/systemd/user/caelestia-shell.service",
            "/whatever",
        ));
        assert!(detect_enabled_units(Path::new("/h"), &diff).is_empty());
    }

    #[test]
    fn empty_diff_returns_empty_list() {
        let got = detect_enabled_units(Path::new("/h"), &FsDiff::default());
        assert!(got.is_empty());
    }
}
