//! pacman integration: snapshot the explicit-install set, diff pre/post,
//! and `-Rns` the added entries on uninstall.
//!
//! We deliberately stay on pacman's explicit set (`-Qqe`) rather than the
//! full installed set (`-Q`). Explicit is user-intent (or rice-intent on
//! install); full set includes dependencies which are pacman's business to
//! track, not ours. On uninstall, `pacman -Rns <added>` removes the
//! explicit packages plus any resulting orphans.

use std::collections::HashSet;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

use super::record::PacmanDiff;

/// Name of the pacman binary. Can be overridden by `RICE_COOKER_PACMAN`
/// (tests) or left as the default.
pub fn pacman_bin() -> String {
    std::env::var("RICE_COOKER_PACMAN").unwrap_or_else(|_| "pacman".into())
}

/// Name of the sudo binary. Overridable for tests.
pub fn sudo_bin() -> String {
    std::env::var("RICE_COOKER_SUDO").unwrap_or_else(|_| "sudo".into())
}

/// A list of explicit-install package names. Order preserved as reported
/// by pacman (alphabetical).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExplicitSet(pub Vec<String>);

impl ExplicitSet {
    pub fn from_lines(s: &str) -> Self {
        let mut v: Vec<String> = s
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect();
        v.sort();
        v.dedup();
        ExplicitSet(v)
    }
    pub fn set(&self) -> HashSet<&str> {
        self.0.iter().map(|s| s.as_str()).collect()
    }
}

/// Run `pacman -Qqe` and parse the output.
pub fn snapshot_explicit() -> Result<ExplicitSet> {
    let out = Command::new(pacman_bin())
        .args(["-Qqe"])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .context("running pacman -Qqe")?;
    if !out.status.success() {
        return Err(anyhow!(
            "pacman -Qqe exited {:?}",
            out.status.code()
        ));
    }
    let s = String::from_utf8(out.stdout).context("pacman -Qqe output not UTF-8")?;
    Ok(ExplicitSet::from_lines(&s))
}

pub fn diff_sets(pre: &ExplicitSet, post: &ExplicitSet) -> PacmanDiff {
    let pre_set = pre.set();
    let post_set = post.set();
    let mut added: Vec<String> = post_set
        .difference(&pre_set)
        .map(|s| s.to_string())
        .collect();
    let mut removed: Vec<String> = pre_set
        .difference(&post_set)
        .map(|s| s.to_string())
        .collect();
    added.sort();
    removed.sort();
    PacmanDiff {
        added_explicit: added,
        removed_explicit: removed,
    }
}

/// Remove the packages added by the install using `sudo pacman -Rns`.
/// `--noconfirm` is forwarded only when `no_confirm` is true — by default
/// the user sees the usual pacman confirmation prompt.
pub fn remove_added(pkgs: &[String], no_confirm: bool) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    let mut cmd = Command::new(sudo_bin());
    cmd.arg(pacman_bin());
    cmd.arg("-Rns");
    if no_confirm {
        cmd.arg("--noconfirm");
    }
    cmd.args(pkgs);
    // Inherit stdin/stdout/stderr so sudo + pacman can prompt the user.
    let status = cmd
        .status()
        .context("spawning sudo pacman -Rns")?;
    if !status.success() {
        return Err(anyhow!(
            "sudo pacman -Rns exited {:?}",
            status.code()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_lines_sorts_dedupes_trims() {
        let s = "beta\nalpha\n\nalpha\n  gamma  \n";
        let e = ExplicitSet::from_lines(s);
        assert_eq!(e.0, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn diff_added_and_removed() {
        let pre = ExplicitSet::from_lines("a\nb\nc");
        let post = ExplicitSet::from_lines("b\nc\nd\ne");
        let d = diff_sets(&pre, &post);
        assert_eq!(d.added_explicit, vec!["d", "e"]);
        assert_eq!(d.removed_explicit, vec!["a"]);
    }

    #[test]
    fn identical_sets_empty_diff() {
        let e = ExplicitSet::from_lines("a\nb");
        let d = diff_sets(&e, &e);
        assert!(d.added_explicit.is_empty());
        assert!(d.removed_explicit.is_empty());
    }
}
