mod common;

use common::{Harness, event_types, last_event, parse_ndjson};

const HAPPY_SHELL_QML: &str = "import QtQuick 2.15\nimport Quickshell\nShellRoot {}\n";

#[test]
fn status_on_empty_cache_reports_defaults() {
    let h = Harness::new();
    let out = h.bin().arg("status").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["active"], serde_json::Value::Null);
    assert_eq!(v["original"], serde_json::Value::Null);
    assert_eq!(v["cache_dir"], h.cache_dir.display().to_string());
    assert!(v["quickshell_running"].is_boolean());
}

#[test]
fn apply_happy_path_updates_active_and_emits_success() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);

    let out = h
        .bin()
        .args([
            "apply",
            "--name",
            "caelestia",
            "--repo",
            "https://example/x.git",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "non-zero exit. stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let events = parse_ndjson(&out.stdout);
    let types = event_types(&events);
    assert_eq!(types.first().copied(), Some("hello"));
    assert_eq!(types.last().copied(), Some("success"));
    assert!(types.contains(&"step"));
    let success = last_event(&events);
    assert_eq!(success["active"], "caelestia");
    assert_eq!(h.read_cache_file("active").as_deref(), Some("caelestia"));
}

#[test]
fn apply_verify_dead_process_reports_qs_exited() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);

    let out = h
        .bin()
        .args([
            "apply",
            "--name",
            "caelestia",
            "--repo",
            "https://example/x.git",
        ])
        .env("FAKE_QS_VERIFY_ALIVE", "0") // pgrep returns not-alive during verify
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "verify");
    assert_eq!(last["reason"], "qs_exited");
}

#[test]
fn apply_rejects_path_traversal_in_name() {
    // The catalog is curated, but a typo'd entry ('..', '/') would otherwise write
    // outside the rices/ subdir. cache::rice_dir guards the filesystem boundary.
    // Names starting with '-' are separately rejected by clap (exit code 2).
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    for bad_name in ["../etc", "..", ".", "foo/bar", ""] {
        let out = h
            .bin()
            .args([
                "apply",
                "--name",
                bad_name,
                "--repo",
                "https://example/x.git",
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(1),
            "name {bad_name:?} should have been rejected"
        );
        let events = parse_ndjson(&out.stdout);
        let last = last_event(&events);
        assert_eq!(last["type"], "fail");
        assert_eq!(last["stage"], "input", "for name {bad_name:?}");
    }
}

#[test]
fn exit_with_no_original_clears_state_and_reports_success() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    // Apply A first so active is set.
    h.bin()
        .args(["apply", "--name", "A", "--repo", "https://example/a.git"])
        .output()
        .unwrap();

    let out = h.bin().arg("exit").output().unwrap();
    assert!(out.status.success());
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "success");
    assert_eq!(h.read_cache_file("active"), None);
}

#[test]
fn apply_rejects_absolute_and_traversal_entry_with_distinct_reason() {
    // An operator-supplied `--entry` that escapes the rice dir gets a dedicated
    // `entry_invalid` fail, not the misleading `entry_missing` path — critical for
    // debugging a misconfigured catalog entry.
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    for bad_entry in ["/etc/passwd", "../escape.qml", "nested/../escape.qml"] {
        let out = h
            .bin()
            .args([
                "apply",
                "--name",
                "x",
                "--repo",
                "https://example/x.git",
                "--entry",
                bad_entry,
            ])
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(1),
            "entry {bad_entry:?} should have been rejected"
        );
        let events = parse_ndjson(&out.stdout);
        let last = last_event(&events);
        assert_eq!(last["type"], "fail");
        assert_eq!(last["stage"], "input", "for entry {bad_entry:?}");
        assert_eq!(last["reason"], "entry_invalid", "for entry {bad_entry:?}");
    }
}

#[test]
fn exit_replays_persisted_argv_via_setsid() {
    // Seeds the cache with a `qs -c clock` original, runs `exit`, and asserts
    // setsid got invoked with the persisted argv — the round-trip the whole
    // OriginalShell feature exists to support.
    let h = Harness::new();
    std::fs::create_dir_all(&h.cache_dir).unwrap();
    std::fs::write(h.cache_dir.join("active"), "somebody\n").unwrap();
    std::fs::write(
        h.cache_dir.join("original"),
        r#"{"argv":["qs","-c","clock"],"cwd":"/tmp"}"#,
    )
    .unwrap();

    let out = h.bin().arg("exit").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let events = parse_ndjson(&out.stdout);
    assert_eq!(last_event(&events)["type"], "success");
    let invocations = std::fs::read_to_string(&h.invocation_log).unwrap();
    assert!(
        invocations.contains("setsid -f qs -c clock"),
        "expected setsid replay, got invocation log:\n{invocations}"
    );
    assert_eq!(h.read_cache_file("active"), None);
}

#[test]
fn apply_overwrites_stale_plain_text_original_file() {
    // A pre-existing `original` from an older backend version (bare path, not JSON)
    // must not break apply — preflight should treat it as unrecorded and overwrite.
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    std::fs::create_dir_all(&h.cache_dir).unwrap();
    std::fs::write(h.cache_dir.join("original"), "shell.qml\n").unwrap();

    let out = h
        .bin()
        .args(["apply", "--name", "x", "--repo", "https://example/x.git"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    // Stale content has been replaced: either empty-sentinel or JSON, not the old path.
    let after = std::fs::read_to_string(h.cache_dir.join("original")).unwrap();
    let trimmed = after.trim();
    assert!(
        trimmed.is_empty() || trimmed.starts_with('{'),
        "expected empty or JSON, got: {after:?}"
    );
}

#[test]
fn apply_falls_back_to_quickshell_subdir_when_root_entry_missing() {
    // Mirrors the DMS layout: entry lives at quickshell/shell.qml, not root.
    // The default `--entry shell.qml` should fall through ENTRY_FALLBACKS and succeed.
    let h = Harness::new();
    h.with_rice_file("quickshell/shell.qml", HAPPY_SHELL_QML);

    let out = h
        .bin()
        .args([
            "apply",
            "--name",
            "dms",
            "--repo",
            "https://example/dms.git",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "non-zero exit. stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    let events = parse_ndjson(&out.stdout);
    assert_eq!(last_event(&events)["type"], "success");
    // Fallback surfaces on stderr so CLI users know which entry was actually used.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("quickshell/shell.qml"),
        "stderr should mention the resolved entry: {stderr}"
    );
}

#[test]
fn second_concurrent_apply_errors_lock_held() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    std::fs::create_dir_all(&h.cache_dir).unwrap();
    let lock_path = h.cache_dir.join("apply.lock");
    use fs4::fs_std::FileExt;
    let f = std::fs::File::create(&lock_path).unwrap();
    f.try_lock_exclusive().unwrap();

    let out = h
        .bin()
        .args(["apply", "--name", "A", "--repo", "https://example/a.git"])
        .output()
        .unwrap();
    let _ = fs4::fs_std::FileExt::unlock(&f);

    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "lock");
    assert_eq!(last["reason"], "already_held");
}
