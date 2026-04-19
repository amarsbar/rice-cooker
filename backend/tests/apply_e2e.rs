mod common;

use common::{event_types, last_event, parse_ndjson, Harness};

const HAPPY_SHELL_QML: &str = "import QtQuick 2.15\nimport Quickshell\nShellRoot {}\n";
const BROKEN_SHELL_QML: &str = "import QtQuick 2.15\nimport Foo.Bar 1.0\nShellRoot {}\n";

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
    assert_eq!(v["previous"], serde_json::Value::Null);
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
fn apply_precheck_failure_does_not_kill_quickshell() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", BROKEN_SHELL_QML);

    let out = h
        .bin()
        .args([
            "apply",
            "--name",
            "busted",
            "--repo",
            "https://example/x.git",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1 (reported failure), not 2 (internal error)"
    );
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "precheck");
    assert_eq!(last["reason"], "missing_plugins");
    assert_eq!(last["plugins"], serde_json::json!(["Foo"]));

    let invocations = h.read_invocations();
    assert!(
        !invocations.contains("pkill -TERM -f ^quickshell"),
        "process state was touched: {invocations}"
    );
    assert!(
        !invocations.contains("setsid"),
        "setsid was invoked: {invocations}"
    );
}

#[test]
fn apply_dry_run_stops_after_precheck() {
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
            "--dry-run",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "success");
    assert_eq!(last["dry_run"], true);
    assert_eq!(last.get("active"), None);

    let invocations = h.read_invocations();
    assert!(
        !invocations.contains("setsid"),
        "dry-run ran setsid: {invocations}"
    );
    // active should NOT be updated
    assert_eq!(h.read_cache_file("active"), None);
}

#[test]
fn apply_no_shell_qml_reports_entry_failure() {
    let h = Harness::new();
    // rice source is empty — no shell.qml anywhere
    let out = h
        .bin()
        .args([
            "apply",
            "--name",
            "empty",
            "--repo",
            "https://example/x.git",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "entry");
    assert_eq!(last["reason"], "no_shell_qml");
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
fn apply_verify_log_error_reports_qs_error() {
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
        .env(
            "FAKE_QS_LOG",
            "QQmlApplicationEngine failed to load component",
        )
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "verify");
    assert_eq!(last["reason"], "qs_error");
    let log_tail = last["log_tail"].as_str().unwrap_or("");
    assert!(
        log_tail.contains("QQmlApplicationEngine"),
        "got: {log_tail}"
    );
}

#[test]
fn revert_with_no_previous_reports_no_previous() {
    let h = Harness::new();
    let out = h.bin().arg("revert").output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "revert");
    assert_eq!(last["reason"], "no_previous");
}

#[test]
fn apply_then_revert_swaps_active_and_previous() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);

    // first apply: A
    let a = h
        .bin()
        .args(["apply", "--name", "A", "--repo", "https://example/a.git"])
        .output()
        .unwrap();
    assert!(
        a.status.success(),
        "{:?}",
        String::from_utf8_lossy(&a.stdout)
    );

    // second apply: B
    let b = h
        .bin()
        .args(["apply", "--name", "B", "--repo", "https://example/b.git"])
        .output()
        .unwrap();
    assert!(b.status.success());
    assert_eq!(h.read_cache_file("active").as_deref(), Some("B"));
    assert_eq!(h.read_cache_file("previous").as_deref(), Some("A"));

    // revert
    let r = h.bin().arg("revert").output().unwrap();
    assert!(
        r.status.success(),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&r.stderr),
        String::from_utf8_lossy(&r.stdout)
    );
    assert_eq!(h.read_cache_file("active").as_deref(), Some("A"));
    assert_eq!(h.read_cache_file("previous").as_deref(), Some("B"));
}

#[test]
fn exit_with_no_original_clears_state_and_reports_success() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    // apply A first so active is set
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
fn apply_records_original_on_first_run_with_no_qs() {
    // First apply with no running quickshell should write an empty `original`.
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    let out = h
        .bin()
        .args(["apply", "--name", "A", "--repo", "https://example/a.git"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let original_path = h.cache_dir.join("original");
    assert!(original_path.is_file());
    let contents = std::fs::read_to_string(&original_path).unwrap();
    assert_eq!(contents, "\n", "expected empty sentinel, got {contents:?}");
}

#[test]
fn apply_clone_failure_reports_clone_stage_with_log_tail() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    let out = h
        .bin()
        .args(["apply", "--name", "x", "--repo", "https://example/x.git"])
        .env("FAKE_GIT_FAIL", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "clone");
    let log_tail = last["log_tail"].as_str().unwrap_or("");
    assert!(
        log_tail.contains("fake git: scripted failure"),
        "log_tail missing git stderr: {log_tail}"
    );
    // Precheck should never have run; no process state touched.
    let invocations = h.read_invocations();
    assert!(
        !invocations.contains("pkill"),
        "pkill was invoked after clone fail: {invocations}"
    );
    assert!(
        !invocations.contains("setsid"),
        "setsid was invoked after clone fail: {invocations}"
    );
}

#[test]
fn apply_setsid_failure_reports_launch_stage() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    let out = h
        .bin()
        .args(["apply", "--name", "x", "--repo", "https://example/x.git"])
        .env("FAKE_SETSID_FAIL", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "launch");
}

#[test]
fn apply_rejects_path_traversal_in_name() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    // Note: names starting with `-` are rejected by clap itself (exit code 2), which is
    // defense-in-depth — our validate_rice_name still rejects them if clap ever changes.
    for bad_name in ["../etc", "..", ".", "foo/bar", "", ".hidden"] {
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
fn apply_rejects_dash_prefixed_repo_url() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    // Use --repo=VALUE to bypass clap's own "value cannot start with -" guard, so the
    // test actually exercises our guard inside git::clone_or_update.
    let out = h
        .bin()
        .args(["apply", "--name", "x", "--repo=--upload-pack=/tmp/evil"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "clone");
    let reason = last["reason"].as_str().unwrap_or("");
    assert!(reason.contains("refusing repo URL"), "reason: {reason}");
}

#[test]
fn apply_sigkill_fallback_fails_cleanly_if_qs_stays_alive() {
    // Simulate the rare case where both TERM and KILL fail to reap quickshell.
    // FAKE_QS_KILL_ALIVE=1 makes the fake pgrep return "alive" for the broad
    // check, so kill_quickshell's post-SIGKILL re-verify should detect that
    // qs is still running and emit a kill_quickshell fail event.
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    let out = h
        .bin()
        .args(["apply", "--name", "x", "--repo", "https://example/x.git"])
        .env("FAKE_QS_KILL_ALIVE", "1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "kill_quickshell");
}

#[test]
fn second_concurrent_apply_errors_lock_held() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", HAPPY_SHELL_QML);
    std::fs::create_dir_all(&h.cache_dir).unwrap();
    let lock_path = h.cache_dir.join("apply.lock");
    // Hold the lock via fs4 directly, then invoke apply.
    use fs4::fs_std::FileExt;
    let f = std::fs::File::create(&lock_path).unwrap();
    f.try_lock_exclusive().unwrap();

    let out = h
        .bin()
        .args(["apply", "--name", "A", "--repo", "https://example/a.git"])
        .output()
        .unwrap();
    // Clean up
    let _ = fs4::fs_std::FileExt::unlock(&f);

    assert_eq!(out.status.code(), Some(1));
    let events = parse_ndjson(&out.stdout);
    let last = last_event(&events);
    assert_eq!(last["type"], "fail");
    assert_eq!(last["stage"], "lock");
    assert_eq!(last["reason"], "already_held");
}
