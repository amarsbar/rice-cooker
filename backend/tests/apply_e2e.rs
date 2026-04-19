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
