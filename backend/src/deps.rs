//! Dep installation via paru/yay. One external process, one GUI polkit
//! prompt via --sudo pkexec. No helper binary, no native AUR handling.

use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};

/// Which AUR helper is available. Preference: paru > yay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    Paru,
    Yay,
}

impl Helper {
    pub fn detect() -> Option<Self> {
        for (name, which) in [("paru", Helper::Paru), ("yay", Helper::Yay)] {
            if Command::new("which")
                .arg(name)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            {
                return Some(which);
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
pub fn check_polkit_agent() -> Result<()> {
    let out = Command::new("pgrep")
        .arg("-f")
        .arg("polkit.*agent|hyprpolkitagent|polkit-gnome|polkit-kde|lxpolkit|lxqt-policykit|mate-polkit")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("running pgrep")?;
    if !out.success() {
        return Err(anyhow!(
            "no polkit authentication agent running. Start one (e.g. `systemctl --user start hyprpolkitagent`) and retry."
        ));
    }
    Ok(())
}

/// Install packages via the AUR helper with pkexec as the sudo backend.
/// paru uses `--sudobin`, yay uses `--sudo`; both take a binary name and
/// invoke it as `<sudobin> pacman -S ...` internally.
///
/// `--needed` skips already-installed packages. `--noconfirm` suppresses
/// paru/yay's own confirmation prompts (not polkit's, which are user-
/// visible). `--skipreview` skips the PKGBUILD diff review paru/yay
/// would otherwise show in an interactive terminal — necessary for GUI
/// operation, standard tradeoff for curated-catalog package managers.
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
        return Err(pkexec_error(status.code(), helper.bin()));
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
        return Err(pkexec_error(status.code(), "pkexec pacman -Rns"));
    }
    Ok(())
}

/// pkexec exit codes: 126 = polkit denied auth (user hit Cancel), 127
/// = pkexec couldn't reach an agent or target binary isn't in the
/// policy. Anything else is forwarded from the underlying command
/// (pacman/paru). Distinguishing these lets the user know whether to
/// re-auth, check their polkit setup, or look at the command output.
fn pkexec_error(code: Option<i32>, label: &str) -> anyhow::Error {
    match code {
        Some(126) => anyhow!("authorization cancelled — re-run and approve the polkit prompt"),
        Some(127) => anyhow!(
            "pkexec could not authorize (no polkit agent reachable or target binary not in policy)"
        ),
        Some(n) => anyhow!("{label} failed (exit {n}) — see output above"),
        None => anyhow!("{label} killed by signal"),
    }
}

/// `pacman -Q <pkg>` — true if installed. Falls back to false on any
/// error (pacman missing, exec failure), so uninstall pre-filters
/// already-removed packages rather than erroring.
pub fn is_installed(pkg: &str) -> bool {
    Command::new("pacman")
        .args(["-Q", pkg])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Subset of `pkgs` that are NOT currently installed. Zero prompts when
/// empty — install flow can skip paru/pkexec entirely.
pub fn missing(pkgs: &[String]) -> Vec<String> {
    pkgs.iter().filter(|p| !is_installed(p)).cloned().collect()
}

/// Subset of `pkgs` that ARE currently installed. Used on uninstall to
/// skip `pacman -Rns` for already-removed entries (which would exit
/// non-zero and block retry).
pub fn installed(pkgs: &[String]) -> Vec<String> {
    pkgs.iter().filter(|p| is_installed(p)).cloned().collect()
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
        assert!(missing(&[]).is_empty());
    }

    #[test]
    fn installed_returns_empty_for_empty_input() {
        assert!(installed(&[]).is_empty());
    }
}
