use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

pub const FAKE_GIT: &str = r#"#!/bin/sh
printf 'git %s\n' "$*" >> "$FAKE_LOG"
# Strip leading top-level options that real git accepts before the subcommand:
# -c KEY=VAL, -C DIR, --version, --exec-path=...
while [ $# -gt 0 ]; do
    case "$1" in
        --version) echo "git version fake 1.0.0"; exit 0 ;;
        -c) shift 2 ;;
        -C) shift 2 ;;
        --exec-path=*|--git-dir=*) shift ;;
        *) break ;;
    esac
done
case "$1" in
    clone)
        # clone [flags...] [--] URL DEST
        shift
        DEST=""
        URL_SEEN=""
        while [ $# -gt 0 ]; do
            case "$1" in
                --depth) shift 2 ;;
                --) shift ;;
                -*) shift ;;
                *)
                    if [ -z "$URL_SEEN" ]; then URL_SEEN=1; shift
                    else DEST="$1"; shift
                    fi
                    ;;
            esac
        done
        if [ -z "$DEST" ]; then exit 1; fi
        if [ -n "$FAKE_GIT_FAIL" ]; then
            echo "fake git: scripted failure" >&2
            exit 1
        fi
        mkdir -p "$DEST/.git"
        if [ -n "$FAKE_RICE_SOURCE" ] && [ -d "$FAKE_RICE_SOURCE" ]; then
            cp -r "$FAKE_RICE_SOURCE"/. "$DEST/"
        fi
        exit 0
        ;;
    fetch|reset)
        exit 0
        ;;
esac
exit 0
"#;

pub const FAKE_PKILL: &str = r#"#!/bin/sh
printf 'pkill %s\n' "$*" >> "$FAKE_LOG"
exit 0
"#;

pub const FAKE_PGREP: &str = r#"#!/bin/sh
printf 'pgrep %s\n' "$*" >> "$FAKE_LOG"
# Hard-override exit code (simulates pgrep 2=syntax / 3=fatal).
if [ -n "$FAKE_PGREP_EXIT" ]; then
    exit "$FAKE_PGREP_EXIT"
fi
# -xf <pattern> = verify check; -f <pattern> = broad kill check
has_xf=0
for a in "$@"; do
    case "$a" in -xf) has_xf=1 ;; esac
done
if [ "$has_xf" = "1" ]; then
    # verify: default alive (exit 0)
    case "${FAKE_QS_VERIFY_ALIVE:-1}" in
        1) exit 0 ;;
        *) exit 1 ;;
    esac
fi
# broad kill check: default not alive (exit 1) so kill loop completes fast
case "${FAKE_QS_KILL_ALIVE:-0}" in
    1) exit 0 ;;
    *) exit 1 ;;
esac
"#;

pub const FAKE_SETSID: &str = r#"#!/bin/sh
printf 'setsid %s\n' "$*" >> "$FAKE_LOG"
# args: -f quickshell -p ./shell.qml — write optional stderr payload to our stderr
if [ -n "$FAKE_QS_LOG" ]; then
    printf '%s\n' "$FAKE_QS_LOG" >&2
fi
if [ -n "$FAKE_SETSID_FAIL" ]; then exit 1; fi
exit 0
"#;

pub const FAKE_QUICKSHELL: &str = r#"#!/bin/sh
printf 'quickshell %s\n' "$*" >> "$FAKE_LOG"
exit 0
"#;

pub struct Harness {
    pub tmp: TempDir,
    pub fakes_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub rice_source: PathBuf,
    pub invocation_log: PathBuf,
}

impl Harness {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let fakes_dir = tmp.path().join("fakes");
        let cache_dir = tmp.path().join("cache");
        let rice_source = tmp.path().join("rice-src");
        let invocation_log = tmp.path().join("invocations.log");
        fs::create_dir_all(&fakes_dir).unwrap();
        fs::create_dir_all(&cache_dir).unwrap();
        fs::create_dir_all(&rice_source).unwrap();
        fs::write(&invocation_log, b"").unwrap();
        for (name, body) in [
            ("git", FAKE_GIT),
            ("pkill", FAKE_PKILL),
            ("pgrep", FAKE_PGREP),
            ("setsid", FAKE_SETSID),
            ("quickshell", FAKE_QUICKSHELL),
        ] {
            let p = fakes_dir.join(name);
            fs::write(&p, body).unwrap();
            let mut perms = fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&p, perms).unwrap();
        }
        Self {
            tmp,
            fakes_dir,
            cache_dir,
            rice_source,
            invocation_log,
        }
    }

    pub fn with_rice_file(&self, rel: &str, body: &str) -> &Self {
        let p = self.rice_source.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
        self
    }

    pub fn bin(&self) -> Command {
        let mut cmd = Command::cargo_bin("rice-cooker-backend").unwrap();
        cmd.env_clear();
        cmd.env(
            "PATH",
            format!("{}:/usr/bin:/bin", self.fakes_dir.display()),
        );
        cmd.env("RICE_COOKER_CACHE_DIR", &self.cache_dir);
        cmd.env("FAKE_LOG", &self.invocation_log);
        cmd.env("FAKE_RICE_SOURCE", &self.rice_source);
        cmd.env("HOME", self.tmp.path());
        cmd
    }

    pub fn read_invocations(&self) -> String {
        fs::read_to_string(&self.invocation_log).unwrap_or_default()
    }

    pub fn read_cache_file(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.cache_dir.join(name))
            .ok()
            .map(|s| s.trim_end().to_string())
    }
}

pub fn parse_ndjson(stdout: &[u8]) -> Vec<serde_json::Value> {
    std::str::from_utf8(stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("bad JSON line: {l:?}: {e}")))
        .collect()
}

pub fn last_event(events: &[serde_json::Value]) -> &serde_json::Value {
    events.last().expect("at least one event expected")
}

pub fn event_types(events: &[serde_json::Value]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e["type"].as_str().unwrap_or("?"))
        .collect()
}

// Silences dead-code warnings for helpers not used in every test file.
#[allow(dead_code)]
fn _unused(_p: &Path) {}
