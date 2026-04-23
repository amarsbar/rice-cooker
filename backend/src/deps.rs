//! Dep installation via paru/yay. One external process, one GUI polkit
//! prompt via --sudo pkexec. No helper binary, no native AUR handling.

use std::env;
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, Result, anyhow};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Helper {
    Paru,
    Yay,
}

impl Helper {
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

/// pkexec without a running agent hangs / silently fails — preflight instead.
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

/// Non-interactive install via paru/yay with pkexec as the sudo backend.
/// paru gets `--skipreview`; yay has no equivalent, so pre-answer its menus as "N".
pub fn install_packages(pkgs: &[String]) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    validate_pkg_names(pkgs)?;
    check_polkit_agent()?;
    let helper = Helper::detect()
        .ok_or_else(|| anyhow!("neither paru nor yay found in PATH — install one first"))?;

    let mut cmd = Command::new(helper.bin());
    match helper {
        Helper::Paru => {
            cmd.args(["--sudobin", "pkexec", "--skipreview"]);
        }
        Helper::Yay => {
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

/// The install record has the exact package list, so skip the helper and go straight to pacman.
pub fn remove_packages(pkgs: &[String]) -> Result<()> {
    if pkgs.is_empty() {
        return Ok(());
    }
    validate_pkg_names(pkgs)?;
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

/// Exit 126/127 are ambiguous: polkit denial/no-agent AND POSIX shell
/// "not executable"/"not found" (e.g. broken pacman hooks). Frame the
/// polkit interpretation as "typical", not definitive.
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

/// Defense-in-depth. The catalog validator already gates what reaches
/// `pacman_deps`/`aur_deps`; this ensures a future catalog-bypassing caller
/// still can't feed arbitrary argv into pkexec-privileged pacman.
fn validate_pkg_names(pkgs: &[String]) -> Result<()> {
    for p in pkgs {
        if p.is_empty() {
            return Err(anyhow!("empty package name rejected"));
        }
        if p.starts_with('-') {
            return Err(anyhow!("refusing package name starting with '-': {p}"));
        }
        if p.contains('/') || p.contains('\\') || p.contains("..") {
            return Err(anyhow!("refusing package name with path characters: {p}"));
        }
        if !p
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-' | '@'))
        {
            return Err(anyhow!(
                "refusing package name with unexpected characters: {p}"
            ));
        }
    }
    Ok(())
}

fn is_executable_file(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(p) {
        Ok(md) => md.is_file() && (md.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
}

/// Distinguishes "not installed" (exit 1) from "pacman is broken" (any other
/// failure). The broken case must propagate — silently collapsing it would
/// make uninstall skip `pacman -Rns` entirely and orphan the rice's packages.
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

pub fn missing(pkgs: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for p in pkgs {
        if !is_installed(p)? {
            out.push(p.clone());
        }
    }
    Ok(out)
}

/// Used on uninstall to skip `pacman -Rns` for already-removed entries (which
/// would exit non-zero and block retry).
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

    #[test]
    fn validate_pkg_names_accepts_real_arch_names() {
        let ok: Vec<String> = [
            "paru",
            "hyprpolkitagent",
            "caelestia-shell-git",
            "gtk+",
            "libc++",
            "lib32-glibc",
            "python-foo.bar",
            "weechat_matrix",
            "pkg@1.0",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert!(validate_pkg_names(&ok).is_ok());
    }

    #[test]
    fn validate_pkg_names_rejects_flag_injection() {
        for bad in ["-rf", "--noconfirm", "-S"] {
            let err = validate_pkg_names(&[bad.to_string()])
                .unwrap_err()
                .to_string();
            assert!(err.contains("starting with '-'"), "got: {err}");
        }
    }

    #[test]
    fn validate_pkg_names_rejects_path_traversal_and_weird_chars() {
        for bad in [
            "../evil",
            "foo/bar",
            "foo\\bar",
            "foo..bar",
            "foo;rm -rf /",
            "foo bar",
            "foo$(whoami)",
            "",
        ] {
            assert!(
                validate_pkg_names(&[bad.to_string()]).is_err(),
                "accepted: {bad:?}"
            );
        }
    }
}
