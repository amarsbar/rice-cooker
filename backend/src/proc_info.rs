use std::path::{Path, PathBuf};

/// Parse the null-separated argv bytes of /proc/<pid>/cmdline into owned Strings.
/// Trailing NUL is tolerated (Linux appends one). Empty input returns an empty Vec.
/// Invalid UTF-8 in an arg becomes a lossy replacement (`String::from_utf8_lossy`).
pub fn parse_cmdline(bytes: &[u8]) -> Vec<String> {
    if bytes.is_empty() {
        return Vec::new();
    }
    // Strip a single trailing NUL so split doesn't produce a spurious empty entry.
    let trimmed = bytes.strip_suffix(b"\0").unwrap_or(bytes);
    trimmed
        .split(|&b| b == 0)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect()
}

/// Given argv (including argv[0]), find the value after `-p` or `-c` (whichever appears first).
/// Skips argv[0] so a binary pathologically named `-p` can't be mistaken for a flag.
/// Returns None if neither flag is present or no value follows.
pub fn extract_entry_arg(argv: &[String]) -> Option<String> {
    let mut iter = argv.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "-p" || arg == "-c" {
            return iter.next().cloned();
        }
    }
    None
}

pub struct QuickshellProc {
    pub pid: i32,
    pub cmdline: Vec<String>,
    /// The process's cwd at scan time (resolved from /proc/<pid>/cwd). Used to
    /// resolve relative `-p` paths back to an absolute path when stamping `original`.
    pub cwd: Option<PathBuf>,
}

/// Scan /proc for the first running process owned by the current UID whose argv[0]
/// basename is exactly "quickshell". Owner-filtering matters on shared hosts:
/// picking up another user's qs and stamping their `-p` path into our `original`
/// file would cause `exit` to later try to launch their session.
pub fn find_running_quickshell() -> anyhow::Result<Option<QuickshellProc>> {
    let our_uid = unsafe { libc::getuid() };
    let proc_dir = std::fs::read_dir("/proc")?;
    for entry in proc_dir {
        // Iterator errors on /proc itself are transient (entry vanishing mid-scan);
        // skip rather than aborting the whole scan.
        let Ok(entry) = entry else { continue };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let pid: i32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !owned_by_uid(pid, our_uid) {
            continue;
        }
        let cmdline_path = format!("/proc/{pid}/cmdline");
        let bytes = match std::fs::read(&cmdline_path) {
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
            let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
            return Ok(Some(QuickshellProc {
                pid,
                cmdline: argv,
                cwd,
            }));
        }
    }
    Ok(None)
}

fn owned_by_uid(pid: i32, uid: libc::uid_t) -> bool {
    let status = match std::fs::read_to_string(format!("/proc/{pid}/status")) {
        Ok(s) => s,
        Err(_) => return false,
    };
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            // Uid: <real> <eff> <saved> <fs>  — first field is what we want.
            if let Some(real) = rest.split_whitespace().next() {
                return real
                    .parse::<libc::uid_t>()
                    .map(|u| u == uid)
                    .unwrap_or(false);
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── parse_cmdline ──────────────────────────────────────────────────────────

    #[test]
    fn parse_cmdline_empty() {
        assert_eq!(parse_cmdline(b""), Vec::<String>::new());
    }

    #[test]
    fn parse_cmdline_trailing_nul() {
        assert_eq!(
            parse_cmdline(b"foo\0bar\0"),
            vec!["foo".to_string(), "bar".to_string()]
        );
    }

    #[test]
    fn parse_cmdline_no_trailing_nul() {
        assert_eq!(
            parse_cmdline(b"foo\0bar"),
            vec!["foo".to_string(), "bar".to_string()]
        );
    }

    #[test]
    fn parse_cmdline_empty_middle_arg() {
        // An empty arg between two NULs must be preserved.
        assert_eq!(
            parse_cmdline(b"foo\0\0bar\0"),
            vec!["foo".to_string(), "".to_string(), "bar".to_string()]
        );
    }

    #[test]
    fn parse_cmdline_invalid_utf8_lossy() {
        // 0xFF is invalid UTF-8; from_utf8_lossy replaces it with U+FFFD.
        let bytes: &[u8] = b"\xff\0ok\0";
        let result = parse_cmdline(bytes);
        assert_eq!(result.len(), 2);
        assert!(result[0].contains('\u{FFFD}'));
        assert_eq!(result[1], "ok");
    }

    // ── extract_entry_arg ──────────────────────────────────────────────────────

    fn sv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn extract_entry_arg_dash_p() {
        assert_eq!(
            extract_entry_arg(&sv(&["quickshell", "-p", "./shell.qml"])),
            Some("./shell.qml".to_string())
        );
    }

    #[test]
    fn extract_entry_arg_dash_c() {
        assert_eq!(
            extract_entry_arg(&sv(&["quickshell", "-c", "mybar"])),
            Some("mybar".to_string())
        );
    }

    #[test]
    fn extract_entry_arg_dash_p_no_value() {
        assert_eq!(extract_entry_arg(&sv(&["quickshell", "-p"])), None);
    }

    #[test]
    fn extract_entry_arg_no_flags() {
        assert_eq!(extract_entry_arg(&sv(&["quickshell"])), None);
    }

    #[test]
    fn extract_entry_arg_unrelated_flag() {
        assert_eq!(extract_entry_arg(&sv(&["quickshell", "--help"])), None);
    }

    #[test]
    fn extract_entry_arg_earliest_wins() {
        // -p comes before -c → -p's value is returned.
        assert_eq!(
            extract_entry_arg(&sv(&["quickshell", "-p", "first", "-c", "second"])),
            Some("first".to_string())
        );
    }
}
