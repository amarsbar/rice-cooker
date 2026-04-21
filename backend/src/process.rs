use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow};

const NOTIFIERS: &[&str] = &["dunst", "mako", "swaync"];
const KILL_POLL_MS: u64 = 50;
const KILL_WAIT_MS: u64 = 500;
// Observed rendering timings on a fast laptop (eDP-1 Hyprland):
//   dms / linux-retroism: <1s to first layer
//   noctalia:             ~3s to first layer (Hyprland IPC discovery,
//                         wallpaper pipeline init, I18n load, etc)
// We verify by polling hyprctl layers / log ERROR / process-alive in a
// loop up to the timeout. Fast rices return in one iteration; slow
// rices get enough runway. Setting the poll interval low keeps
// happy-path rice switches snappy.
const VERIFY_POLL_MS: u64 = 250;
const VERIFY_TIMEOUT_MS: u64 = 5000;
const LOG_TAIL_LINES: usize = 20;

// Match both `quickshell` and its `qs` symlink (shipped by the same package), with
// or without a leading absolute/relative path. Users invoke any of: `qs -c <name>`,
// `quickshell -p …`, or `/usr/bin/quickshell …`. `( |$)` after the name guards
// against `qsfoo`/`quickshellx` false positives; `(^|/)` before handles the path
// prefix case while still anchoring at a component boundary.
pub const QS_MATCH_PATTERN: &str = r"(^|/)(quickshell|qs)( |$)";

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
    pgrep_matches(&["-f", QS_MATCH_PATTERN])
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

/// Return the PIDs matched by `pgrep <args>`. Empty vec on no match; Err on
/// syntax / fatal. Shared exit-code discipline with `pgrep_matches`.
fn pgrep_pids(args: &[&str]) -> Result<Vec<u32>> {
    let out = Command::new("pgrep")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .context("spawning pgrep")?;
    match out.status.code() {
        Some(0) => {
            let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(|l| l.trim().parse::<u32>().ok())
                .collect();
            Ok(pids)
        }
        Some(1) => Ok(Vec::new()),
        Some(c) => Err(anyhow!("pgrep {:?} failed with exit code {}", args, c)),
        None => Err(anyhow!("pgrep {:?} terminated by signal", args)),
    }
}

pub fn launch_detached(rice_dir: &Path, entry_rel: &Path, log_file: &Path) -> Result<()> {
    let argv = vec![
        "quickshell".to_string(),
        "-p".to_string(),
        format!("./{}", entry_rel.display()),
    ];
    launch_argv(&argv, rice_dir, log_file)
}

/// Relaunch a shell from a persisted argv+cwd pair. Used by `exit` to restore the
/// user's pre-RiceCooker shell regardless of how it was invoked (`-p <path>` or
/// `-c <name>`), since we keep the full argv we scraped from /proc.
pub fn launch_argv(argv: &[String], cwd: &Path, log_file: &Path) -> Result<()> {
    let (argv0, rest) = argv
        .split_first()
        .ok_or_else(|| anyhow!("empty argv; nothing to launch"))?;
    let log = fs::File::create(log_file)
        .with_context(|| format!("opening log {}", log_file.display()))?;
    // `setsid -f` forks a new session leader and returns immediately — its exit code
    // reflects only whether setsid *spawned* successfully, NOT whether the child
    // stayed alive. For the apply path, health is checked by `verify()`; for the
    // exit/restore path we trust the user's prior shell to come back up.
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

pub fn verify(rice_dir: &Path, entry_rel: &Path, log_file: &Path) -> Result<VerifyResult> {
    let pat = quickshell_cmdline_pattern(entry_rel);
    let entry_abs = rice_dir.join(entry_rel);
    let deadline = std::time::Instant::now() + Duration::from_millis(VERIFY_TIMEOUT_MS);
    let mut hypr_ever_said_no = false; // hyprctl answered Some(false) at least once
    let mut hypr_ever_said_yes_or_missing = false; // Some(true) or None ever

    // Poll loop. First signal wins; on timeout we decide based on what
    // hyprctl was saying over the whole window.
    loop {
        thread::sleep(Duration::from_millis(VERIFY_POLL_MS));

        let pids = pgrep_pids(&["-xf", &pat]).unwrap_or_default();
        let alive = !pids.is_empty();

        let log_contents = find_quickshell_runtime_log(&entry_abs)
            .and_then(|p| fs::read_to_string(&p).ok())
            .or_else(|| fs::read_to_string(log_file).ok())
            .unwrap_or_default();

        // Process died → Dead.
        if !alive {
            return Ok(VerifyResult::Dead {
                log_tail: tail_lines_or_placeholder(&log_contents, &entry_abs),
            });
        }
        // Log says "Failed to load configuration" → Dead. This is the
        // specific marker quickshell emits when top-level QML load fails;
        // the "caused by" chain follows it. We deliberately do NOT match
        // on "ERROR:" alone — quickshell emits that prefix for
        // Qt-deprecation notices and non-fatal runtime errors (e.g.
        // `ERROR: qs:@... should be coerced to void`) that don't stop
        // the shell from rendering.
        if log_contents.contains("Failed to load configuration") {
            return Ok(VerifyResult::Dead {
                log_tail: tail_lines_or_placeholder(&log_contents, &entry_abs),
            });
        }
        // Compositor check. Some(true) = definitively rendering;
        // Some(false) = hyprctl answered "no layers for this PID" —
        // keep polling (layers may appear later); None = hyprctl
        // unavailable, won't be useful on retries either.
        match hyprland_owns_layers(&pids) {
            Some(true) => return Ok(VerifyResult::Ok),
            Some(false) => hypr_ever_said_no = true,
            None => hypr_ever_said_yes_or_missing = true,
        }

        if std::time::Instant::now() >= deadline {
            // Time's up. Decide based on what hyprctl was saying:
            // - If it consistently said "no layers" across the whole
            //   window, we have high confidence the shell is not
            //   rendering → Dead with a synthetic explanation.
            // - If it was never reachable (None throughout), we have no
            //   surface-level signal at all; fall back to "alive +
            //   log-clean = Ok", matching the pre-hyprctl behavior on
            //   non-Hyprland compositors.
            if hypr_ever_said_no && !hypr_ever_said_yes_or_missing {
                let base_tail = tail_lines_or_placeholder(&log_contents, &entry_abs);
                let msg = format!(
                    "{base_tail}\n<rice-cooker: shell alive + log-clean but created 0 layer-shell surfaces in {}ms — the rice likely needs a runtime dep (wallpaper path, dbus service, specific env) that isn't present>",
                    VERIFY_TIMEOUT_MS
                );
                return Ok(VerifyResult::Dead { log_tail: msg });
            }
            return Ok(VerifyResult::Ok);
        }
    }
}

fn tail_lines_or_placeholder(log: &str, entry_abs: &Path) -> String {
    if log.is_empty() {
        format!("<no log content for {}>", entry_abs.display())
    } else {
        tail_lines(log, LOG_TAIL_LINES)
    }
}

/// Return `Some(true)` if Hyprland's layer-shell list contains any surface
/// owned by one of the given PIDs. `Some(false)` if Hyprland answered
/// definitively that none of those PIDs own a layer. `None` if we
/// couldn't ask (hyprctl not on PATH, JSON unparseable, not a Hyprland
/// session) — the caller treats None as "no signal" and proceeds.
fn hyprland_owns_layers(pids: &[u32]) -> Option<bool> {
    let out = Command::new("hyprctl")
        .args(["layers", "-j"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let body = String::from_utf8(out.stdout).ok()?;
    let root: serde_json::Value = serde_json::from_str(&body).ok()?;
    // Shape: { "<monitor>": { "levels": { "0": [ {pid, ...}, ... ], ... } } }
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

/// Locate the `log.log` quickshell wrote for a specific entry path by scanning
/// `$XDG_RUNTIME_DIR/quickshell/by-id/*`. Picks the newest log whose content
/// references the target path — that filter is what keeps us from returning
/// some other running shell's log if the user has concurrent Quickshells.
fn find_quickshell_runtime_log(entry_abs: &Path) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let by_id = PathBuf::from(runtime).join("quickshell/by-id");
    find_qs_log_in(&by_id, entry_abs)
}

fn find_qs_log_in(by_id: &Path, entry_abs: &Path) -> Option<PathBuf> {
    let target = entry_abs.to_string_lossy().into_owned();
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(by_id).ok()?.flatten() {
        let log = entry.path().join("log.log");
        let Ok(meta) = fs::metadata(&log) else {
            continue;
        };
        let Ok(mtime) = meta.modified() else { continue };
        let Ok(content) = fs::read_to_string(&log) else {
            continue;
        };
        if !content.contains(&target) {
            continue;
        }
        if best.as_ref().is_none_or(|(t, _)| mtime > *t) {
            best = Some((mtime, log));
        }
    }
    best.map(|(_, p)| p)
}

// Hardcoded to `quickshell` (not `qs`): this pattern only verifies a shell *we*
// launched via `launch_detached`, which always uses argv[0] = "quickshell". The
// broader `QS_MATCH_PATTERN` above is for finding/killing arbitrary user-invoked
// shells, which is a different question.
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

pub struct QuickshellProc {
    pub cmdline: Vec<String>,
    /// /proc/<pid>/cwd at scan time. Persisted with argv so `exit` can relaunch
    /// with the same working directory — needed when the shell was started with
    /// a relative `-p` path.
    pub cwd: Option<PathBuf>,
}

/// Scan /proc for a process whose argv[0] basename is "quickshell" or its `qs`
/// symlink. Returns the first match in /proc iteration order, which is
/// kernel-dependent — relying on a *specific* match when two shells run
/// simultaneously is wrong. The design doc assumes one at a time, so first-match
/// is fine here.
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
    fn find_qs_log_picks_newest_matching_entry_ignores_unrelated() {
        use std::time::Duration;
        let tmp = tempfile::tempdir().unwrap();
        let by_id = tmp.path();
        let target = Path::new("/some/path/shell.qml");

        // Mkdir helper: write a log.log with given content and bump mtime offset.
        let write_log = |id: &str, body: &str| {
            let dir = by_id.join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("log.log"), body).unwrap();
            dir.join("log.log")
        };

        // Unrelated entry — a different shell, mentions a different path.
        let _older = write_log("aaa111", "INFO: Launching /other/shell.qml\nERROR: oops");
        // Older match for our target.
        let older_match = write_log(
            "bbb222",
            "INFO: Launching config: /some/path/shell.qml\nERROR: old run",
        );
        // Newer match for our target — should win.
        std::thread::sleep(Duration::from_millis(20));
        let newer_match = write_log(
            "ccc333",
            "INFO: Launching config: /some/path/shell.qml\nERROR: new run",
        );

        let found = find_qs_log_in(by_id, target).expect("should find a match");
        assert_eq!(found, newer_match);
        // Older match exists but isn't picked.
        assert!(older_match.is_file());

        // No match when no log mentions the target.
        let unrelated_target = Path::new("/nowhere/shell.qml");
        assert!(find_qs_log_in(by_id, unrelated_target).is_none());
    }

    #[test]
    fn qs_match_pattern_matches_both_names_and_rejects_false_positives() {
        let re = regex::Regex::new(QS_MATCH_PATTERN).expect("compiles");
        // Accepts: bare names, path-prefixed invocations, both qs and quickshell,
        // with and without trailing args.
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
        // Rejects: same prefix with extra letters, names embedded in other words,
        // or appearing mid-cmdline without a component boundary.
        for cmdline in [
            "quickshellx -p foo",
            "qsfoo",
            "/usr/bin/qsfoo -c x",
            "foo quickshell bar",
        ] {
            assert!(!re.is_match(cmdline), "should not match: {cmdline}");
        }
    }
}
