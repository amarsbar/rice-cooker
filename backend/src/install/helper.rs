//! pkexec invocation wrapper for `rice-cooker-helper`.
//!
//! The helper lives in its own crate (`rice-cooker-helper/`). This module
//! shells out to `pkexec <helper> <subcommand> <args>`. It never touches
//! pacman directly — that's the helper's job, the whole point of the
//! split.
//!
//! Helper path resolution:
//! - `$RICE_COOKER_HELPER_BIN` if set (for tests + pre-install dev).
//! - `/usr/bin/rice-cooker-helper` otherwise.
//!
//! Polkit agent preflight: `pkexec` without a running auth agent
//! silently hangs / fails. We `pgrep` for a known agent before calling,
//! so failure surfaces as a clear error instead of a stuck process.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Result, anyhow};

/// pgrep regex covering the common polkit authentication agents. Not
/// exhaustive — `busctl call polkitd ListTemporaryAuthorizations` would
/// be authoritative but adds D-Bus complexity. Heuristic is fine for v1;
/// the cost of a false positive (user has a polkit-*-like process that
/// doesn't actually handle auth) is one hung pkexec call on a real bug.
const POLKIT_AGENT_REGEX: &str =
    "polkit.*agent|hyprpolkitagent|polkit-gnome|polkit-kde|lxpolkit|lxqt-policykit|mate-polkit";

pub fn helper_bin() -> PathBuf {
    std::env::var_os("RICE_COOKER_HELPER_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/usr/bin/rice-cooker-helper"))
}

/// Check if a polkit authentication agent is running. `pkexec` needs one
/// to prompt the user; without it, our call either fails silently or
/// hangs. Return `Err` with a clear message so the caller aborts before
/// invoking pkexec.
pub fn check_polkit_agent() -> Result<()> {
    let out = Command::new("pgrep")
        .arg("-f")
        .arg(POLKIT_AGENT_REGEX)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| anyhow!("running pgrep: {e}"))?;
    if !out.success() {
        return Err(anyhow!(
            "no polkit authentication agent running. Start one (e.g. `systemctl --user start hyprpolkitagent`) and retry."
        ));
    }
    Ok(())
}

pub fn install_repo_packages(pkgs: &[String]) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    check_polkit_agent()?;
    run_helper("install-repo-packages", pkgs)
}

pub fn install_built_packages(paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    check_polkit_agent()?;
    let args: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    run_helper("install-built-packages", &args)
}

pub fn remove_packages(pkgs: &[String]) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    check_polkit_agent()?;
    run_helper("remove-packages", pkgs)
}

fn run_helper(subcommand: &str, args: &[String]) -> Result<()> {
    let helper = helper_bin();
    if !Path::new(&helper).exists() {
        return Err(anyhow!(
            "rice-cooker-helper not found at {}. Build + install it, or set RICE_COOKER_HELPER_BIN.",
            helper.display()
        ));
    }
    let status = Command::new("pkexec")
        .arg(&helper)
        .arg(subcommand)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| anyhow!("spawning pkexec {}: {e}", helper.display()))?;
    if !status.success() {
        return Err(anyhow!(
            "pkexec {} {} exited {:?}",
            helper.display(),
            subcommand,
            status.code()
        ));
    }
    Ok(())
}

/// Return the subset of `pkgs` that are NOT currently installed per
/// `pacman -Q`. Used by install-time preflight to skip pkexec entirely
/// when all deps are already present — that's the zero-prompt
/// happy-path for rices whose deps are baselined in the PKGBUILD.
pub fn missing_pacman(pkgs: &[String]) -> Vec<String> {
    pkgs.iter()
        .filter(|p| !super::pacman::is_installed(p).unwrap_or(false))
        .cloned()
        .collect()
}
