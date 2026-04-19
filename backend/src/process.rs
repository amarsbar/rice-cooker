use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use regex::Regex;

const NOTIFIERS: &[&str] = &["dunst", "mako", "swaync"];
const KILL_POLL_MS: u64 = 50;
const KILL_WAIT_MS: u64 = 500;
const VERIFY_WAIT_MS: u64 = 300;
const LOG_TAIL_LINES: usize = 20;

// Ordered most-specific-first: scan_log_for_errors returns the first match, so the more
// diagnostic patterns (e.g. the exact missing module name) should win over generic ones
// like "QQmlApplicationEngine failed" that often accompany them on the same log.
const ERROR_PATTERNS: &[&str] = &[
    r#"module "[^"]*" is not installed"#,
    r"Cannot assign to non-existent property",
    r"Component is not ready",
    r"\bSyntaxError\b",
    r"QQmlApplicationEngine failed",
];

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
        return Err(anyhow!("setsid exited non-zero: {status}"));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub enum VerifyResult {
    Ok,
    Dead { log_tail: String },
    LogErrors { log_tail: String },
}

pub fn verify(entry_rel: &Path, log_file: &Path) -> Result<VerifyResult> {
    thread::sleep(Duration::from_millis(VERIFY_WAIT_MS));
    let pat = quickshell_cmdline_pattern(entry_rel);
    let alive = pgrep_matches(&["-xf", &pat])?;
    // Surfacing log-read failures matters here: verify's whole job is diagnosis.
    // If we silently fell back to "", a missing-log bug would masquerade as a healthy shell.
    let log_contents = match fs::read_to_string(log_file) {
        Ok(s) => s,
        Err(e) => format!("<log unreadable at {}: {}>", log_file.display(), e),
    };
    let log_tail = tail_lines(&log_contents, LOG_TAIL_LINES);
    if !alive {
        return Ok(VerifyResult::Dead { log_tail });
    }
    if scan_log_for_errors(&log_contents).is_some() {
        return Ok(VerifyResult::LogErrors { log_tail });
    }
    Ok(VerifyResult::Ok)
}

pub fn quickshell_cmdline_pattern(entry_rel: &Path) -> String {
    let s = entry_rel.display().to_string();
    format!(r"^quickshell -p \./{}$", regex::escape(&s))
}

pub fn scan_log_for_errors(log: &str) -> Option<String> {
    for re in error_regexes() {
        if let Some(m) = re.find(log) {
            return Some(m.as_str().to_string());
        }
    }
    None
}

fn error_regexes() -> &'static [Regex] {
    static CACHE: OnceLock<Vec<Regex>> = OnceLock::new();
    CACHE.get_or_init(|| {
        ERROR_PATTERNS
            .iter()
            .map(|p| Regex::new(p).expect("valid error regex"))
            .collect()
    })
}

pub fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_escapes_dots_and_slashes() {
        let p = quickshell_cmdline_pattern(Path::new("ii/shell.qml"));
        assert_eq!(p, r"^quickshell -p \./ii/shell\.qml$");
        Regex::new(&p).expect("compiles");
    }

    #[test]
    fn scan_returns_none_for_clean_log() {
        assert!(scan_log_for_errors("").is_none());
        assert!(scan_log_for_errors("qs: loaded ok\n").is_none());
    }

    #[test]
    fn scan_detects_known_error_patterns() {
        assert_eq!(
            scan_log_for_errors("QQmlApplicationEngine failed to load").as_deref(),
            Some("QQmlApplicationEngine failed")
        );
        let missing = r#"module "Foo.Bar" is not installed"#;
        assert!(scan_log_for_errors(missing).is_some());
    }

    #[test]
    fn tail_returns_last_n_lines() {
        assert_eq!(tail_lines("1\n2\n3\n4\n5", 2), "4\n5");
        assert_eq!(tail_lines("a\nb\nc", 10), "a\nb\nc");
    }
}
