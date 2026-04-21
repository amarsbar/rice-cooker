//! Run the rice's install_cmd.
//!
//! `sh -c "<install_cmd>"` in the clone dir, with stdin/stdout/stderr
//! routed per catalog's `interactive` flag:
//!   interactive=false → stdin=/dev/null; stdout+stderr teed to log file
//!     AND the parent's stderr so the user sees progress.
//!   interactive=true  → stdin/stdout/stderr inherited from parent tty,
//!     and the log file captures a simulated header/footer so we have a
//!     record.
//!
//! Exit code is returned; we do NOT fail on non-zero — the pipeline treats
//! non-zero exit as a "partial install" signal.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Execute `install_cmd` in `clone_dir`, capturing output to `log_path`.
/// Returns the exit code (negative if signal-terminated).
pub fn run_install_cmd(
    clone_dir: &Path,
    install_cmd: &str,
    interactive: bool,
    log_path: &Path,
    extra_env: &[(String, String)],
) -> Result<i32> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating log dir {}", parent.display()))?;
    }
    // Open the log file once up-front; we write a header + stream output
    // into it.
    let mut log = fs::File::create(log_path)
        .with_context(|| format!("creating log file {}", log_path.display()))?;
    writeln!(
        log,
        "### rice-cooker install log\n### cwd: {}\n### cmd: {}\n### interactive: {}\n",
        clone_dir.display(),
        install_cmd,
        interactive
    )
    .ok();

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(install_cmd).current_dir(clone_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }

    let status = if interactive {
        // Full tty passthrough. Caller's stdout/stderr go to user; we can't
        // also tee into the log. Write a header noting we didn't capture.
        writeln!(log, "### (interactive mode; output not captured)").ok();
        drop(log);
        cmd.stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("running install_cmd (interactive)")?
    } else {
        // Non-interactive: stdin=/dev/null, tee stdout+stderr to both log
        // and parent's stderr. We stream via piped + a reader thread for
        // each stream. Simpler alternative: rely on `tee(1)` via the shell.
        // Use shell-level redirection: `sh -c "<cmd> 2>&1 | tee -a <log>"`
        // so we inherit the pipeline semantics for free. However that
        // changes the exit code to tee's. Use bash's pipefail instead.
        let piped_cmd = format!(
            "set -o pipefail 2>/dev/null; ({}) 2>&1 | tee -a {}",
            install_cmd,
            shell_quote(log_path)
        );
        drop(log);
        Command::new("bash")
            .arg("-c")
            .arg(&piped_cmd)
            .current_dir(clone_dir)
            .envs(extra_env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .context("running install_cmd (piped)")?
    };

    // Unix: `status.code()` is None when signal-killed. Represent that as a
    // negative code so callers can still record *something* deterministic.
    match status.code() {
        Some(c) => Ok(c),
        None => {
            use std::os::unix::process::ExitStatusExt;
            let sig = status.signal().unwrap_or(0);
            Ok(-sig)
        }
    }
}

fn shell_quote(p: &Path) -> String {
    // Single-quote and escape any embedded single quotes. Good enough for
    // paths we generate ourselves (no embedded quotes in practice).
    let s = p.display().to_string();
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// The path inside `dirs.logs_dir()` where a log for this invocation
/// should land. Includes a unix timestamp so repeated installs of the
/// same rice don't overwrite each other.
pub fn log_path(logs_dir: &Path, name: &str) -> PathBuf {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    logs_dir.join(format!("{name}-{ts}.log"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn runs_simple_command_and_captures_exit_code() {
        let tmp = tempdir().unwrap();
        let log = tmp.path().join("log");
        let code = run_install_cmd(
            tmp.path(),
            "true",
            false,
            &log,
            &[],
        )
        .unwrap();
        assert_eq!(code, 0);
        let body = fs::read_to_string(&log).unwrap();
        assert!(body.contains("rice-cooker install log"));
    }

    #[test]
    fn non_zero_exit_is_reported_not_failed() {
        let tmp = tempdir().unwrap();
        let log = tmp.path().join("log");
        let code = run_install_cmd(
            tmp.path(),
            "exit 7",
            false,
            &log,
            &[],
        )
        .unwrap();
        assert_eq!(code, 7);
    }

    #[test]
    fn output_is_captured_to_log_in_non_interactive() {
        let tmp = tempdir().unwrap();
        let log = tmp.path().join("log");
        let code = run_install_cmd(
            tmp.path(),
            "echo hello world; echo oops >&2",
            false,
            &log,
            &[],
        )
        .unwrap();
        assert_eq!(code, 0);
        let body = fs::read_to_string(&log).unwrap();
        assert!(body.contains("hello world"), "log missing stdout: {body}");
        assert!(body.contains("oops"), "log missing stderr: {body}");
    }

    #[test]
    fn extra_env_is_exported_to_install_cmd() {
        let tmp = tempdir().unwrap();
        let log = tmp.path().join("log");
        let code = run_install_cmd(
            tmp.path(),
            "echo \"${RICE_COOKER_TEST_VAR:-missing}\"",
            false,
            &log,
            &[("RICE_COOKER_TEST_VAR".into(), "yes".into())],
        )
        .unwrap();
        assert_eq!(code, 0);
        let body = fs::read_to_string(&log).unwrap();
        assert!(body.contains("yes"), "env not exported: {body}");
    }
}
