use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};

const NOTIFIERS: &[&str] = &["dunst", "mako", "swaync"];
const KILL_POLL_MS: u64 = 50;
const KILL_WAIT_MS: u64 = 500;
const VERIFY_WAIT_MS: u64 = 300;
const LOG_TAIL_LINES: usize = 20;

pub fn kill_notif_daemons() -> Result<()> {
    for name in NOTIFIERS {
        run_pkill(&["-TERM", "-x", name])?;
    }
    Ok(())
}

pub fn kill_quickshell() -> Result<()> {
    run_pkill(&["-TERM", "-f", "^quickshell"])?;

    let deadline = Instant::now() + Duration::from_millis(KILL_WAIT_MS);
    while Instant::now() < deadline {
        if !quickshell_running()? {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(KILL_POLL_MS));
    }

    run_pkill(&["-KILL", "-f", "^quickshell"])?;
    // Give SIGKILL a moment to land, then confirm the process is actually gone —
    // otherwise step 6 would race a still-alive quickshell and the user would see
    // two shells fighting over layer-shell surfaces.
    thread::sleep(Duration::from_millis(KILL_POLL_MS));
    if quickshell_running()? {
        return Err(anyhow!(
            "quickshell still running after SIGKILL (possibly D-state)"
        ));
    }
    Ok(())
}

// pkill exit codes: 0 = matched and signaled, 1 = no processes matched, 2 = syntax, 3 = fatal.
// We accept 0 and 1 as success; anything else (including spawn failure) is a real error.
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
    pgrep_matches(&["-f", "^quickshell"])
}

// pgrep exit codes: 0 = matches found, 1 = no matches, 2 = syntax, 3 = fatal.
// Match the same discipline as run_pkill: conflating 2/3 with 1 (no-match) would
// silently let a broken pgrep invocation report "not running" and bypass the
// post-SIGKILL re-verify.
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

pub fn launch_detached(rice_dir: &Path, entry_rel: &Path, log_file: &Path) -> Result<()> {
    let log = fs::File::create(log_file)
        .with_context(|| format!("opening log {}", log_file.display()))?;
    let entry_arg = format!("./{}", entry_rel.display());
    // `setsid -f` forks a new session leader and returns immediately — its exit code
    // reflects only whether setsid *spawned* successfully, NOT whether the child
    // quickshell stayed alive. Quickshell health is checked separately by `verify()`.
    let status = Command::new("setsid")
        .arg("-f")
        .arg("quickshell")
        .arg("-p")
        .arg(&entry_arg)
        .current_dir(rice_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log)
        .status()
        .context("spawning setsid quickshell")?;
    if !status.success() {
        return Err(anyhow!(
            "setsid failed to spawn (exit {status}); quickshell health is checked by verify()"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerifyResult {
    Ok,
    Dead { log_tail: String },
}

pub fn verify(entry_rel: &Path, log_file: &Path) -> Result<VerifyResult> {
    thread::sleep(Duration::from_millis(VERIFY_WAIT_MS));
    let pat = quickshell_cmdline_pattern(entry_rel);
    let alive = pgrep_matches(&["-xf", &pat])?;
    if alive {
        return Ok(VerifyResult::Ok);
    }
    // Surface log-read failures in the tail instead of silently falling back to "".
    let log_contents = match fs::read_to_string(log_file) {
        Ok(s) => s,
        Err(e) => format!("<log unreadable at {}: {}>", log_file.display(), e),
    };
    Ok(VerifyResult::Dead {
        log_tail: tail_lines(&log_contents, LOG_TAIL_LINES),
    })
}

pub fn quickshell_cmdline_pattern(entry_rel: &Path) -> String {
    let s = entry_rel.display().to_string();
    format!(r"^quickshell -p \./{}$", regex::escape(&s))
}

pub fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

// ── /proc introspection: used to record the user's pre-RiceCooker shell ───────

/// Parse the null-separated argv bytes of /proc/<pid>/cmdline into owned Strings.
/// Trailing NUL is tolerated (Linux appends one). Invalid UTF-8 becomes U+FFFD.
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

/// Given argv (including argv[0]), find the value after `-p`. Only `-p` is matched —
/// `-c <name>` takes a config name, not a path, so recording it and later launching
/// as `quickshell -p ./<name>` on `exit` would silently load the wrong thing.
pub fn extract_entry_arg(argv: &[String]) -> Option<String> {
    let mut iter = argv.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "-p" {
            return iter.next().cloned();
        }
    }
    None
}

pub struct QuickshellProc {
    pub cmdline: Vec<String>,
    /// /proc/<pid>/cwd at scan time, used to resolve relative `-p` paths when
    /// stamping `original`.
    pub cwd: Option<PathBuf>,
}

/// Scan /proc for a process whose argv[0] basename is exactly "quickshell".
/// Returns the first match in /proc iteration order, which is kernel-dependent —
/// relying on a *specific* match when two Quickshells run simultaneously is wrong.
/// The design doc assumes one Quickshell at a time, so first-match is fine here.
///
/// No owner filtering: on single-user Linux laptops (our target) there's only one
/// quickshell. `hidepid`, where enabled, masks other users' /proc entries; where
/// it isn't enabled we'd pick up another user's qs on a shared host — a threat
/// model v0 doesn't defend against.
pub fn find_running_quickshell() -> Result<Option<QuickshellProc>> {
    let proc_dir = fs::read_dir("/proc")?;
    for entry in proc_dir {
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let Ok(pid) = name.to_string_lossy().parse::<i32>() else {
            continue;
        };
        let bytes = match fs::read(format!("/proc/{pid}/cmdline")) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let argv = parse_cmdline(&bytes);
        if argv.is_empty() {
            continue;
        }
        let argv0_basename = Path::new(&argv[0])
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if argv0_basename == "quickshell" {
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
    fn pattern_escapes_dots_and_slashes() {
        let p = quickshell_cmdline_pattern(Path::new("ii/shell.qml"));
        assert_eq!(p, r"^quickshell -p \./ii/shell\.qml$");
        regex::Regex::new(&p).expect("compiles");
    }

    #[test]
    fn tail_returns_last_n_lines() {
        assert_eq!(tail_lines("1\n2\n3\n4\n5", 2), "4\n5");
        assert_eq!(tail_lines("a\nb\nc", 10), "a\nb\nc");
    }

    #[test]
    fn parse_cmdline_handles_edge_cases() {
        assert!(parse_cmdline(b"").is_empty());
        // Trailing NUL tolerated; missing NUL OK; empty middle arg preserved.
        assert_eq!(parse_cmdline(b"foo\0bar\0"), vec!["foo", "bar"]);
        assert_eq!(parse_cmdline(b"foo\0bar"), vec!["foo", "bar"]);
        assert_eq!(parse_cmdline(b"foo\0\0bar\0"), vec!["foo", "", "bar"]);
        // Invalid UTF-8 becomes U+FFFD via from_utf8_lossy.
        let lossy = parse_cmdline(b"\xff\0ok\0");
        assert!(lossy[0].contains('\u{FFFD}'));
        assert_eq!(lossy[1], "ok");
    }

    #[test]
    fn extract_entry_arg_covers_every_shape() {
        fn sv(xs: &[&str]) -> Vec<String> {
            xs.iter().map(|s| s.to_string()).collect()
        }
        // argv[0] is always skipped; only `-p` matches.
        let cases: &[(&[&str], Option<&str>)] = &[
            (&["quickshell", "-p", "./shell.qml"], Some("./shell.qml")),
            (&["quickshell", "-c", "mybar"], None),
            (&["quickshell", "-p"], None),
            (&["quickshell"], None),
            (&["quickshell", "--help"], None),
            (
                &["quickshell", "-c", "first", "-p", "second"],
                Some("second"),
            ),
        ];
        for (argv, expected) in cases {
            assert_eq!(
                extract_entry_arg(&sv(argv)).as_deref(),
                *expected,
                "argv = {argv:?}"
            );
        }
    }
}
