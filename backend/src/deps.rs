//! Dep installation via paru/yay. One external process, one GUI polkit
//! prompt via --sudo pkexec. No helper binary, no native AUR handling.

use std::env;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, Result, anyhow};

/// Which AUR helper is available. Preference: paru > yay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    Paru,
    Yay,
}

impl Helper {
    /// Walk `$PATH` directly rather than shelling out to `which` —
    /// avoids a fork per lookup AND avoids false "not found" reports
    /// on minimal systems where `which` itself is missing.
    pub fn detect() -> Option<Self> {
        let path = env::var_os("PATH")?;
        for (name, helper) in [("paru", Helper::Paru), ("yay", Helper::Yay)] {
            for dir in env::split_paths(&path) {
                if is_executable_file(&dir.join(name)) {
                    return Some(helper);
                }
            }
        }
        None
    }

    pub fn bin(self) -> &'static str {
        match self {
            Helper::Paru => "paru",
            Helper::Yay => "yay",
        }
    }
}

/// Best-effort polkit-agent preflight. pkexec without a running agent
/// hangs / silently fails; detect and surface a clear error instead.
///
/// pgrep exits 0 on match, 1 on no-match; anything else is pgrep
/// itself breaking (syntax error, /proc restricted, EPERM from
/// sandboxing). Treat those as "environment unknown" with a distinct
/// error so the user doesn't chase a missing polkit agent that's
/// actually running.
pub fn check_polkit_agent() -> Result<()> {
    let status = Command::new("pgrep")
        .arg("-f")
        .arg("polkit.*agent|hyprpolkitagent|polkit-gnome|polkit-kde|lxpolkit|lxqt-policykit|mate-polkit")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("running pgrep")?;
    if let Some(sig) = status.signal() {
        return Err(anyhow!(
            "pgrep killed by signal {sig} while checking for a polkit agent"
        ));
    }
    match status.code() {
        Some(0) => Ok(()),
        Some(1) => Err(anyhow!(
            "no polkit authentication agent running. Start one (e.g. `systemctl --user start hyprpolkitagent`) and retry."
        )),
        Some(n) => Err(anyhow!(
            "pgrep returned unexpected exit {n} while checking for a polkit agent; cannot verify authorization environment"
        )),
        None => Err(anyhow!(
            "pgrep exited without a status code while checking for a polkit agent"
        )),
    }
}

/// Install packages via the AUR helper with pkexec as the sudo backend.
/// paru uses `--sudobin`, yay uses `--sudo`; both take a binary name and
/// invoke it as `<sudobin> pacman -S ...` internally.
///
/// `--needed` skips already-installed packages. `--noconfirm` suppresses
/// paru/yay's own confirmation prompts (not polkit's, which are user-
/// visible).
///
/// Review-prompt suppression is helper-specific:
/// * paru gets `--skipreview` (one flag skips all PKGBUILD diff menus).
/// * yay has no `--skipreview`; it gets `--answerclean/diff/edit/upgrade N`
///   which pre-answers each of yay's individual menus as "no", so the
///   menu renders and auto-advances without user input.
///
/// Needed for GUI operation; standard tradeoff for a curated catalog
/// where the pinned commit is reviewed upstream, not per install.
pub fn install_packages(pkgs: &[String]) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    check_polkit_agent()?;
    let helper = Helper::detect()
        .ok_or_else(|| anyhow!("neither paru nor yay found in PATH — install one first"))?;

    let mut cmd = Command::new(helper.bin());
    // Flag shape is helper-specific. Paru has --skipreview (skips all
    // review menus). Yay has separate menu-suppression flags plus
    // --answer* to pre-answer each menu as "no" — we set both for
    // belt-and-suspenders non-interactivity.
    match helper {
        Helper::Paru => {
            cmd.args(["--sudobin", "pkexec", "--skipreview"]);
        }
        Helper::Yay => {
            // yay has no --skipreview; menu flags have no `no` variants.
            // Pre-answer each menu as "N" so the menu renders and
            // auto-advances without user input.
            cmd.args([
                "--sudo",
                "pkexec",
                "--answerclean",
                "N",
                "--answerdiff",
                "N",
                "--answeredit",
                "N",
                "--answerupgrade",
                "N",
            ]);
        }
    }
    cmd.args(["-S", "--needed", "--noconfirm"]);
    cmd.args(pkgs);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .with_context(|| format!("spawning {}", helper.bin()))?;
    if !status.success() {
        return Err(pkexec_error(status, helper.bin()));
    }
    Ok(())
}

/// Remove packages via pkexec pacman directly. No helper needed for
/// removal — we have the exact list from the install record, no dep
/// resolution required.
pub fn remove_packages(pkgs: &[String]) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    check_polkit_agent()?;
    let status = Command::new("pkexec")
        .args(["pacman", "-Rns", "--noconfirm"])
        .args(pkgs)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("spawning pkexec pacman")?;
    if !status.success() {
        return Err(pkexec_error(status, "pkexec pacman -Rns"));
    }
    Ok(())
}

/// Map a failed pkexec-driven command's exit status to a human error.
///
/// Exit codes 126/127 are ambiguous: they CAN indicate polkit
/// outcomes (126 = action denied / not in policy, 127 = no agent
/// reachable) but they're also POSIX shell conventions for "found
/// but not executable" / "not found" — which pacman hooks, broken
/// post-install scripts, or missing `execve` targets will surface
/// through paru unchanged. So we always include the label + exit
/// code and frame the polkit interpretation as "typical", not
/// definitive. Signal terminations include the signal number instead
/// of silently collapsing to "killed".
///
/// `check_polkit_agent` preflights the common "no agent" case, so
/// 127 in practice usually means a real pacman-hook breakage.
fn pkexec_error(status: ExitStatus, label: &str) -> anyhow::Error {
    if let Some(sig) = status.signal() {
        return anyhow!("{label} killed by signal {sig}");
    }
    match status.code() {
        Some(126) => anyhow!(
            "{label} failed (exit 126): typically polkit denied or cancelled authorization — \
             if you approved the prompt, scan the output above for the real error"
        ),
        Some(127) => anyhow!(
            "{label} failed (exit 127): typically no polkit agent reachable — \
             if one is running, this is usually a pacman hook invoking a missing binary \
             (scan the output above)"
        ),
        Some(n) => anyhow!("{label} failed (exit {n}) — see output above"),
        None => anyhow!("{label} exited without a status code"),
    }
}

/// True if `p` exists, is a regular file, and has at least one exec
/// bit set. Used by `Helper::detect` to avoid relying on `which`.
fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(md) => md.is_file() && (md.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
}

/// `pacman -Q <pkg>` — Ok(true) if installed, Ok(false) if the pkg
/// isn't present. Distinguishes "not installed" (pacman exit 1) from
/// "pacman itself is broken" (any other exit / exec failure). The
/// broken-pacman case propagates as Err so callers can fail-stop
/// instead of silently collapsing to "nothing needs action" — which
/// on uninstall would skip `pacman -Rns` entirely and leave the
/// rice's packages on disk with no tool-visible owner.
pub fn is_installed(pkg: &str) -> Result<bool> {
    let status = Command::new("pacman")
        .args(["-Q", pkg])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("running pacman -Q")?;
    if let Some(sig) = status.signal() {
        return Err(anyhow!("pacman -Q {pkg} killed by signal {sig}"));
    }
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(n) => Err(anyhow!(
            "pacman -Q {pkg} returned unexpected exit {n}; pacman may be broken (DB lock, corrupt state)"
        )),
        None => Err(anyhow!("pacman -Q {pkg} exited without a status code")),
    }
}

/// Subset of `pkgs` that are NOT currently installed. Propagates
/// pacman-query errors so an install against a broken pacman aborts
/// instead of trying to re-install the full dep set.
pub fn missing(pkgs: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for p in pkgs {
        if !is_installed(p)? {
            out.push(p.clone());
        }
    }
    Ok(out)
}

/// Subset of `pkgs` that ARE currently installed. Used on uninstall
/// to skip `pacman -Rns` for already-removed entries (which would
/// exit non-zero and block retry). Propagates pacman-query errors so
/// uninstall against a broken pacman aborts rather than silently
/// skipping package removal.
pub fn installed(pkgs: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for p in pkgs {
        if is_installed(p)? {
            out.push(p.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_bin_names() {
        assert_eq!(Helper::Paru.bin(), "paru");
        assert_eq!(Helper::Yay.bin(), "yay");
    }

    #[test]
    fn missing_returns_empty_for_empty_input() {
        assert!(missing(&[]).unwrap().is_empty());
    }

    #[test]
    fn installed_returns_empty_for_empty_input() {
        assert!(installed(&[]).unwrap().is_empty());
    }
}
