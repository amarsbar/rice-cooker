use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};

const NOTIFIERS: &[&str] = &["dunst", "mako", "swaync"];
const KILL_POLL_MS: u64 = 50;
const KILL_WAIT_MS: u64 = 500;
// Observed rendering timings on a fast laptop (eDP-1 Hyprland):
//   dms / linux-retroism: <1s to first layer
//   noctalia:             ~3s to first layer
// Fast rices return in one iteration; slow rices get enough runway.
const VERIFY_POLL_MS: u64 = 250;
const VERIFY_TIMEOUT_MS: u64 = 5000;
const LOG_TAIL_LINES: usize = 20;

// Match both `quickshell` and its `qs` symlink, with or without a leading
// path. `( |$)` after the name guards against `qsfoo`/`quickshellx` false
// positives; `(^|/)` before handles the path prefix case.
pub const QS_MATCH_PATTERN: &str = r"(^|/)(quickshell|qs)( |$)";

/// True when `quickshell -c <name>` has a running process. Uses the same
/// cmdline literal verify_by_name emits so the pattern stays in sync.
pub fn rice_shell_alive(name: &str) -> Result<bool> {
    let pat = format!("quickshell -c {}", regex::escape(name));
    pgrep_matches(&["-xf", &pat])
}

pub fn kill_notif_daemons() -> Result<()> {
    for name in NOTIFIERS {
        run_pkill(&["-TERM", "-x", name])?;
    }
    Ok(())
}

pub fn kill_quickshell() -> Result<()> {
    run_pkill(&["-TERM", "-f", QS_MATCH_PATTERN])?;

    let deadline = Instant::now() + Duration::from_millis(KILL_WAIT_MS);
    while Instant::now() < deadline {
        if !quickshell_running()? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(KILL_POLL_MS));
    }

    run_pkill(&["-KILL", "-f", QS_MATCH_PATTERN])?;
    // Re-verify after SIGKILL: `quickshell --no-duplicate` is the default, so
    // our follow-up launch would exit immediately (and silently) if a previous
    // qs is still alive. Returning Err here lets the caller surface the cause.
    thread::sleep(Duration::from_millis(KILL_POLL_MS));
    if quickshell_running()? {
        return Err(anyhow!(
            "quickshell still running after SIGKILL (possibly D-state)"
        ));
    }
    Ok(())
}

// pkill exits 0 = matched, 1 = no match, 2 = syntax, 3 = fatal. Treat 0/1 as
// success; anything else is a real error.
fn run_pkill(args: &[&str]) -> Result<()> {
    let status = Command::new("pkill")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("spawning pkill")?;
    match status.code() {
        Some(0) | Some(1) => Ok(()),
        Some(c) => Err(anyhow!("pkill {:?} failed with exit code {}", args, c)),
        None => Err(anyhow!("pkill {:?} terminated by signal", args)),
    }
}

fn quickshell_running() -> Result<bool> {
    pgrep_matches(&["-f", QS_MATCH_PATTERN])
}

// pgrep exit codes mirror pkill. Conflating syntax/fatal with no-match would
// silently bypass the post-SIGKILL re-verify.
fn pgrep_matches(args: &[&str]) -> Result<bool> {
    let status = Command::new("pgrep")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("spawning pgrep")?;
    match status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        Some(c) => Err(anyhow!("pgrep {:?} failed with exit code {}", args, c)),
        None => Err(anyhow!("pgrep {:?} terminated by signal", args)),
    }
}

fn pgrep_pids(args: &[&str]) -> Result<Vec<u32>> {
    let out = Command::new("pgrep")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .context("spawning pgrep")?;
    match out.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| l.trim().parse::<u32>().ok())
            .collect()),
        Some(1) => Ok(Vec::new()),
        Some(c) => Err(anyhow!("pgrep {:?} failed with exit code {}", args, c)),
        None => Err(anyhow!("pgrep {:?} terminated by signal", args)),
    }
}

/// Launch `quickshell -c <name>` as a detached session leader. Quickshell
/// resolves `<name>` against `$XDG_CONFIG_HOME/quickshell/<name>/shell.qml`,
/// which is the target of the symlink our install pipeline creates.
pub fn launch_detached_by_name(name: &str, log_file: &Path, cwd: &Path) -> Result<()> {
    let argv = vec!["quickshell".to_string(), "-c".to_string(), name.to_string()];
    launch_argv(&argv, cwd, log_file)
}

/// Relaunch from a persisted argv+cwd pair. Used by uninstall to restore the
/// user's pre-rice shell regardless of how it was invoked (`-p <path>` or
/// `-c <name>`).
pub fn launch_argv(argv: &[String], cwd: &Path, log_file: &Path) -> Result<()> {
    let (argv0, rest) = argv
        .split_first()
        .ok_or_else(|| anyhow!("empty argv; nothing to launch"))?;
    let log = fs::File::create(log_file)
        .with_context(|| format!("opening log {}", log_file.display()))?;
    // setsid's exit reflects spawn success only; child health is checked by
    // `verify_by_name`.
    let status = Command::new("setsid")
        .arg("-f")
        .arg(argv0)
        .args(rest)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .status()
        .with_context(|| format!("spawning setsid {argv0}"))?;
    if !status.success() {
        return Err(anyhow!("setsid failed to spawn (exit {status})"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerifyResult {
    Ok,
    Dead { log_tail: String },
}

/// Poll up to VERIFY_TIMEOUT_MS for `quickshell -c <name>` alive + log-clean
/// + (on Hyprland) owning a layer-shell surface.
pub fn verify_by_name(name: &str, log_file: &Path) -> Result<VerifyResult> {
    // regex::escape: catalog allows '.', '+', etc. in names (e.g. `foo.1`);
    // pgrep -f treats the pattern as a regex, so unescaped metas would match
    // laxly (`foo.1` matches `fooX1`). `-x` anchors; escape handles the rest.
    let pat = format!("quickshell -c {}", regex::escape(name));
    let deadline = Instant::now() + Duration::from_millis(VERIFY_TIMEOUT_MS);
    let mut hypr_ever_said_no = false;

    loop {
        thread::sleep(Duration::from_millis(VERIFY_POLL_MS));

        let pids = pgrep_pids(&["-xf", &pat])?;
        let alive = !pids.is_empty();
        let log_contents = fs::read_to_string(log_file).unwrap_or_default();

        if !alive {
            return Ok(VerifyResult::Dead {
                log_tail: tail_lines_or_placeholder(&log_contents, name),
            });
        }
        // Specific marker quickshell emits on top-level QML load failure.
        // Deliberately NOT matching bare "ERROR:" — quickshell emits that
        // prefix for Qt deprecation notices and non-fatal runtime errors.
        if log_contents.contains("Failed to load configuration") {
            return Ok(VerifyResult::Dead {
                log_tail: tail_lines_or_placeholder(&log_contents, name),
            });
        }
        match hyprland_owns_layers(&pids) {
            Some(true) => return Ok(VerifyResult::Ok),
            Some(false) => hypr_ever_said_no = true,
            None => {}
        }

        if Instant::now() >= deadline {
            // Re-check liveness: `alive` above is up to VERIFY_POLL_MS stale.
            if !pgrep_matches(&["-xf", &pat])? {
                return Ok(VerifyResult::Dead {
                    log_tail: tail_lines_or_placeholder(&log_contents, name),
                });
            }
            // Alive + log-clean + hyprctl said "no layers" ⇒ rice is up but
            // not rendering. Non-Hyprland compositor leaves hypr_ever_said_no
            // false and falls back to alive + log-clean = Ok.
            if hypr_ever_said_no {
                let base_tail = tail_lines_or_placeholder(&log_contents, name);
                return Ok(VerifyResult::Dead {
                    log_tail: format!(
                        "{base_tail}\n<rice-cooker: shell alive + log-clean but created 0 layer-shell surfaces in {VERIFY_TIMEOUT_MS}ms — likely a missing runtime dep (wallpaper path, dbus service, specific env)>"
                    ),
                });
            }
            return Ok(VerifyResult::Ok);
        }
    }
}

fn tail_lines_or_placeholder(log: &str, name: &str) -> String {
    if log.is_empty() {
        format!("<no log content for quickshell -c {name}>")
    } else {
        tail_lines(log, LOG_TAIL_LINES)
    }
}

/// Some(answer) if hyprctl responded; None on any failure. `timeout` so a
/// wedged compositor can't block past verify's deadline.
fn hyprland_owns_layers(pids: &[u32]) -> Option<bool> {
    let out = Command::new("timeout")
        .args(["--signal=KILL", "1", "hyprctl", "layers", "-j"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let body = String::from_utf8(out.stdout).ok()?;
    let root: serde_json::Value = serde_json::from_str(&body).ok()?;
    // Shape: { "<monitor>": { "levels": { "0": [ {pid, ...}, ... ] } } }
    let root_obj = root.as_object()?;
    let pid_set: std::collections::HashSet<u32> = pids.iter().copied().collect();
    for monitor in root_obj.values() {
        let Some(levels) = monitor.get("levels").and_then(|v| v.as_object()) else {
            continue;
        };
        for layer_list in levels.values() {
            let Some(arr) = layer_list.as_array() else {
                continue;
            };
            for layer in arr {
                if let Some(pid) = layer.get("pid").and_then(|v| v.as_u64())
                    && pid_set.contains(&(pid as u32))
                {
                    return Some(true);
                }
            }
        }
    }
    Some(false)
}

pub fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

// ── /proc introspection: record the user's pre-rice shell ─────────────────────

/// Parse null-separated argv bytes from /proc/<pid>/cmdline. Trailing NUL
/// tolerated (Linux appends one). Invalid UTF-8 becomes U+FFFD.
pub fn parse_cmdline(bytes: &[u8]) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let trimmed = bytes.strip_suffix(b"\0").unwrap_or(bytes);
    trimmed
        .split(|&b| b == 0)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect()
}

pub struct QuickshellProc {
    pub cmdline: Vec<String>,
    /// cwd preserved for relative `-p` paths in the original argv.
    pub cwd: Option<PathBuf>,
}

/// First /proc entry whose argv[0] basename is `quickshell` or `qs`.
pub fn find_running_quickshell() -> Result<Option<QuickshellProc>> {
    for entry in fs::read_dir("/proc")? {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<i32>() else {
            continue;
        };
        // NotFound = process exited between readdir and read (common).
        // PermissionDenied = hidepid or another user's entry (skip).
        // Anything else = propagate — if our own qs is unreadable for a weird
        // reason, we shouldn't silently record "nothing was running".
        let bytes = match fs::read(format!("/proc/{pid}/cmdline")) {
            Ok(b) => b,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                ) =>
            {
                continue;
            }
            Err(e) => return Err(anyhow!("reading /proc/{pid}/cmdline: {e}")),
        };
        let argv = parse_cmdline(&bytes);
        if argv.is_empty() {
            continue;
        }
        let argv0_basename = Path::new(&argv[0])
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if matches!(argv0_basename.as_str(), "quickshell" | "qs") {
            let cwd = fs::read_link(format!("/proc/{pid}/cwd")).ok();
            return Ok(Some(QuickshellProc { cmdline: argv, cwd }));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_returns_last_n_lines() {
        assert_eq!(tail_lines("1\n2\n3\n4\n5", 2), "4\n5");
        assert_eq!(tail_lines("a\nb\nc", 10), "a\nb\nc");
    }

    #[test]
    fn parse_cmdline_handles_edge_cases() {
        assert!(parse_cmdline(b"").is_empty());
        assert_eq!(parse_cmdline(b"foo\0bar\0"), vec!["foo", "bar"]);
        assert_eq!(parse_cmdline(b"foo\0bar"), vec!["foo", "bar"]);
        assert_eq!(parse_cmdline(b"foo\0\0bar\0"), vec!["foo", "", "bar"]);
        let lossy = parse_cmdline(b"\xff\0ok\0");
        assert!(lossy[0].contains('\u{FFFD}'));
        assert_eq!(lossy[1], "ok");
    }

    #[test]
    fn qs_match_pattern_matches_both_names_and_rejects_false_positives() {
        let re = regex::Regex::new(QS_MATCH_PATTERN).unwrap();
        for cmdline in [
            "quickshell",
            "quickshell -p ./shell.qml",
            "qs",
            "qs -c clock",
            "/usr/bin/quickshell -p ./shell.qml",
            "/usr/bin/qs -c clock",
            "./qs -c clock",
        ] {
            assert!(re.is_match(cmdline), "should match: {cmdline}");
        }
        for cmdline in ["quickshellx -p foo", "qsfoo", "/usr/bin/qsfoo -c x"] {
            assert!(!re.is_match(cmdline), "should not match: {cmdline}");
        }
    }
}
