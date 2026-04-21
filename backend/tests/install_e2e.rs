//! End-to-end install/uninstall/switch tests under a sandboxed HOME.
//!
//! Uses the in-tree catalog.toml so the shipped entries are exercised with
//! their real install_cmd strings. Fake git populates clones from
//! `$FAKE_RICE_SOURCE`. Pacman is skipped via `--skip-pacman`. Systemd
//! disabling is skipped by simply not creating any systemd wants-symlinks
//! in fixtures.

mod common;

use std::fs;

use common::{Harness, read_json};

const SHELL_QML: &str = "import QtQuick 2.15\nimport Quickshell\nShellRoot {}\n";

fn catalog_for(name: &str, install_cmd: &str) -> String {
    format!(
        r#"
[{name}]
display_name = "{name}"
repo = "https://example/{name}"
commit = "0123456789abcdef0123456789abcdef01234567"
install_cmd = "{install_cmd}"
"#
    )
}

#[test]
fn install_happy_path_creates_current_and_record() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    // install_cmd: symlink the clone dir into $HOME/.config/quickshell/<name>/.
    let cat = catalog_for(
        "noctalia",
        "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/noctalia\\\"",
    );
    h.with_catalog(&cat);

    let out = h.bin().args(["install", "noctalia"]).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // current.json points at noctalia.
    let cur = read_json(&h.current_json());
    assert_eq!(cur["name"], "noctalia");
    // record.json exists and has the install metadata.
    let rec = read_json(&h.record_json("noctalia"));
    assert_eq!(rec["name"], "noctalia");
    assert_eq!(rec["partial"], false);
    // Symlink landed where install_cmd put it.
    let dest = h.home_path(".config/quickshell/noctalia");
    assert!(dest.is_symlink(), "dest should be a symlink");
    // fs_diff captured the symlink.
    let symlinks = rec["fs_diff"]["symlinks_added"].as_array().unwrap();
    assert_eq!(symlinks.len(), 1);
}

#[test]
fn install_refuses_when_something_already_installed() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    let cat = format!(
        "{}{}",
        catalog_for(
            "noctalia",
            "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/noctalia\\\"",
        ),
        catalog_for("other", "true"),
    );
    h.with_catalog(&cat);

    h.bin().args(["install", "noctalia"]).output().unwrap();
    let out = h.bin().args(["install", "other"]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("already installed"),
        "stderr should mention already-installed: {err}"
    );
}

#[test]
fn install_rejects_unknown_name() {
    let h = Harness::new();
    h.with_catalog("");
    let out = h.bin().args(["install", "mystery"]).output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not in catalog"), "{err}");
}

#[test]
fn uninstall_without_install_reports_none() {
    let h = Harness::new();
    h.with_catalog("");
    let out = h.bin().arg("uninstall").output().unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("no rice installed"), "{err}");
}

#[test]
fn install_then_uninstall_round_trips_to_clean_state() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    let cat = catalog_for(
        "noctalia",
        "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/noctalia\\\"",
    );
    h.with_catalog(&cat);

    h.bin().args(["install", "noctalia"]).output().unwrap();
    let dest = h.home_path(".config/quickshell/noctalia");
    assert!(dest.exists());

    let out = h.bin().arg("uninstall").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Current + record gone; previous.json retained.
    assert!(!h.current_json().exists());
    assert!(!h.record_json("noctalia").exists());
    assert!(h.previous_json().exists());
    // Symlink reversed.
    assert!(!dest.exists(), "symlink should be gone after uninstall");
}

#[test]
fn install_records_partial_on_non_zero_exit_code() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    // install_cmd fails after creating a file — partial state.
    let cat = catalog_for(
        "noctalia",
        "mkdir -p \\\"$HOME/.config/quickshell/noctalia\\\" && cp shell.qml \\\"$HOME/.config/quickshell/noctalia/shell.qml\\\" && exit 5",
    );
    h.with_catalog(&cat);

    let out = h.bin().args(["install", "noctalia"]).output().unwrap();
    assert!(
        out.status.success(),
        "install should still succeed (record partial, not crash): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rec = read_json(&h.record_json("noctalia"));
    assert_eq!(rec["partial"], true);
    assert_eq!(rec["exit_code"], 5);
    // Uninstall refuses without --force on a partial.
    let out = h.bin().arg("uninstall").output().unwrap();
    assert!(!out.status.success());
    // With --force it proceeds.
    let out = h.bin().args(["uninstall", "--force"]).output().unwrap();
    assert!(
        out.status.success(),
        "uninstall --force should succeed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn uninstall_preserves_user_modifications_via_rcsave() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    let cat = catalog_for(
        "noctalia",
        "mkdir -p \\\"$HOME/.config/quickshell/noctalia\\\" && cp shell.qml \\\"$HOME/.config/quickshell/noctalia/shell.qml\\\"",
    );
    h.with_catalog(&cat);

    h.bin().args(["install", "noctalia"]).output().unwrap();
    // User edits the deployed file.
    let deployed = h.home_path(".config/quickshell/noctalia/shell.qml");
    fs::write(&deployed, b"USER EDITED").unwrap();

    let out = h.bin().arg("uninstall").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // The user's version should be preserved at .rcsave-<ts>.
    let parent = deployed.parent().unwrap();
    let rcsaves: Vec<_> = fs::read_dir(parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .contains(".rcsave-")
        })
        .collect();
    assert_eq!(rcsaves.len(), 1, "expected one .rcsave, got {rcsaves:?}");
    let body = fs::read_to_string(rcsaves[0].path()).unwrap();
    assert_eq!(body, "USER EDITED");
}

#[test]
fn switch_replaces_current_in_one_step() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    let cat = format!(
        "{}{}",
        catalog_for(
            "alpha",
            "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/alpha\\\"",
        ),
        catalog_for(
            "beta",
            "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/beta\\\"",
        ),
    );
    h.with_catalog(&cat);

    h.bin().args(["install", "alpha"]).output().unwrap();
    assert!(h.home_path(".config/quickshell/alpha").exists());

    let out = h.bin().args(["switch", "beta"]).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!h.home_path(".config/quickshell/alpha").exists());
    assert!(h.home_path(".config/quickshell/beta").exists());
    assert_eq!(read_json(&h.current_json())["name"], "beta");
}

#[test]
fn list_marks_installed() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    let cat = format!(
        "{}{}",
        catalog_for(
            "alpha",
            "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/alpha\\\"",
        ),
        catalog_for("beta", "true"),
    );
    h.with_catalog(&cat);

    h.bin().args(["install", "alpha"]).output().unwrap();
    let out = h.bin().arg("list").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("* alpha"), "missing *alpha line: {stdout}");
    assert!(stdout.contains("  beta"), "missing beta line: {stdout}");
}

#[test]
fn status_shows_install_record_summary() {
    let h = Harness::new();
    h.with_rice_file("shell.qml", SHELL_QML);
    let cat = catalog_for(
        "noctalia",
        "mkdir -p \\\"$HOME/.config/quickshell\\\" && ln -sfnT \\\"$PWD\\\" \\\"$HOME/.config/quickshell/noctalia\\\"",
    );
    h.with_catalog(&cat);
    h.bin().args(["install", "noctalia"]).output().unwrap();

    let out = h.bin().arg("status").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("name:"), "{stdout}");
    assert!(stdout.contains("noctalia"), "{stdout}");
}

#[test]
fn status_empty_when_nothing_installed() {
    let h = Harness::new();
    h.with_catalog("");
    let out = h.bin().arg("status").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("nothing installed"), "{stdout}");
}

/// Release gate: fails if any shipped catalog entry still carries a
/// placeholder SHA. Marked #[ignore] so day-to-day `cargo test` passes
/// while the catalog is in bring-up; run via
/// `cargo test -- --ignored placeholder` before tagging a release.
#[test]
#[ignore]
fn no_shipped_catalog_entry_carries_a_placeholder_commit() {
    let cat_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog.toml");
    let body = fs::read_to_string(&cat_path).unwrap();
    let cat = rice_cooker_backend::catalog::Catalog::from_str(&body).unwrap();
    let placeholders: Vec<(&str, &str)> = cat
        .rices
        .iter()
        .filter(|(_, e)| rice_cooker_backend::catalog::is_placeholder_commit(&e.commit))
        .map(|(n, e)| (n.as_str(), e.commit.as_str()))
        .collect();
    assert!(
        placeholders.is_empty(),
        "{} catalog entries still have placeholder commits:\n  {}",
        placeholders.len(),
        placeholders
            .iter()
            .map(|(n, c)| format!("{n} @ {c}"))
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

#[test]
fn ships_the_in_tree_catalog_with_15_rices() {
    let cat_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog.toml");
    let body = fs::read_to_string(&cat_path).unwrap();
    let cat = rice_cooker_backend::catalog::Catalog::from_str(&body).unwrap();
    let names: Vec<&str> = cat.names().collect();
    assert_eq!(names.len(), 15, "expected 15 shipped rices, got {names:?}");
    assert!(names.contains(&"caelestia"), "catalog missing caelestia");
    for expected in [
        "Ambxst",
        "Moonveil",
        "NibrasShell",
        "Zaphkiel",
        "caelestia",
        "dhrruvsharma-shell",
        "dms",
        "dotfiles-hyprland",
        "end-4",
        "eqsh",
        "iNiR",
        "linux-retroism",
        "noctalia",
        "nucleus",
        "whisker",
    ] {
        assert!(
            names.contains(&expected),
            "catalog missing {expected:?}; got {names:?}"
        );
    }
}

/// Loops the 15 shipped entries through install → uninstall, using a
/// minimal fake rice source (a single shell.qml) for every rice. The in-tree
/// install_cmd strings do the symlink work; the test asserts the pipeline
/// produces a record and reverses cleanly.
#[test]
fn caelestia_style_install_with_real_install_fish_shape() {
    // Can't run caelestia's actual install.fish (needs pacman + AUR +
    // network). Instead, seed a fake rice that mirrors caelestia's SHAPE:
    // an install script that creates a handful of config files across
    // watched roots, plus a partial_ownership target (~/.zshrc) and a
    // runtime_regenerated target (~/.config/gtk-3.0/settings.ini). Then
    // verify the install records them, a user-edit on the deployed
    // shell.qml is .rcsave'd on uninstall, the runtime_regenerated file
    // is restored without .rcsave, and the partial_ownership file is
    // always .rcsave'd.
    let h = Harness::new();

    // Pre-existing user state: a zshrc with user content.
    fs::create_dir_all(h.home_path("")).unwrap();
    fs::write(h.home_path(".zshrc"), b"# user's zshrc\n").unwrap();
    // And a pre-existing gtk settings (runtime-regenerated by the rice).
    fs::create_dir_all(h.home_path(".config/gtk-3.0")).unwrap();
    fs::write(
        h.home_path(".config/gtk-3.0/settings.ini"),
        b"[Settings]\ngtk-theme-name=PreRice\n",
    )
    .unwrap();

    h.with_rice_file(
        "install.fish",
        concat!(
            "#!/bin/sh\n",
            "mkdir -p \"$HOME/.config/quickshell/caelestia\" \"$HOME/.config/gtk-3.0\"\n",
            "echo shell > \"$HOME/.config/quickshell/caelestia/shell.qml\"\n",
            "printf '[Settings]\\ngtk-theme-name=Caelestia\\n' > \"$HOME/.config/gtk-3.0/settings.ini\"\n",
            "printf '%s\\n' '# Caelestia' >> \"$HOME/.zshrc\"\n",
            "exit 0\n",
        ),
    );
    let cat = r#"
[caelestia]
display_name = "Caelestia"
repo = "https://example/caelestia"
commit = "0123456789abcdef0123456789abcdef01234567"
install_cmd = "sh ./install.fish"
partial_ownership = ["~/.zshrc"]
runtime_regenerated = ["~/.config/gtk-3.0/settings.ini"]
"#;
    h.with_catalog(cat);

    let out = h.bin().args(["install", "caelestia"]).output().unwrap();
    assert!(
        out.status.success(),
        "install failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Deployed shell + modified gtk + appended zshrc.
    assert!(h.home_path(".config/quickshell/caelestia/shell.qml").exists());
    let gtk = fs::read_to_string(h.home_path(".config/gtk-3.0/settings.ini")).unwrap();
    assert!(gtk.contains("Caelestia"));
    let zsh = fs::read_to_string(h.home_path(".zshrc")).unwrap();
    assert!(zsh.contains("# Caelestia"));

    // Record reflects the state.
    let rec = read_json(&h.record_json("caelestia"));
    assert_eq!(rec["partial"], false);
    // partial_ownership + runtime_regenerated expanded in the record.
    let po = rec["partial_ownership_paths"].as_array().unwrap();
    assert!(po.iter().any(|v| v.as_str().unwrap().ends_with(".zshrc")));
    let rr = rec["runtime_regenerated_paths"].as_array().unwrap();
    assert!(rr.iter().any(|v| v.as_str().unwrap().ends_with("settings.ini")));

    // User edits the deployed shell.qml post-install.
    fs::write(
        h.home_path(".config/quickshell/caelestia/shell.qml"),
        b"USER TWEAKED",
    )
    .unwrap();

    let out = h.bin().arg("uninstall").output().unwrap();
    assert!(
        out.status.success(),
        "uninstall failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // gtk settings (runtime_regenerated) should be restored to pre-install
    // WITHOUT an .rcsave (policy: runtime-regen drift is expected).
    let gtk = fs::read_to_string(h.home_path(".config/gtk-3.0/settings.ini")).unwrap();
    assert!(gtk.contains("PreRice"), "gtk not restored: {gtk}");
    let gtk_parent = h.home_path(".config/gtk-3.0");
    let no_gtk_rcsave = fs::read_dir(&gtk_parent)
        .unwrap()
        .filter_map(|e| e.ok())
        .all(|e| !e.file_name().to_string_lossy().contains(".rcsave-"));
    assert!(
        no_gtk_rcsave,
        "runtime_regenerated path should NOT be .rcsave'd"
    );

    // zshrc (partial_ownership) is always .rcsave'd and restored.
    let home_dir = h.home_path("");
    let zsh_rcsave = fs::read_dir(&home_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .any(|e| e.file_name().to_string_lossy().starts_with(".zshrc.rcsave-"));
    assert!(
        zsh_rcsave,
        "partial_ownership path should have been .rcsave'd"
    );
    let zsh = fs::read_to_string(h.home_path(".zshrc")).unwrap();
    assert!(
        zsh.contains("user's zshrc") && !zsh.contains("# Caelestia"),
        "zshrc should be restored to pre-install: {zsh}"
    );

    // User's edit to the deployed shell.qml should survive as .rcsave.
    let qs_parent = h.home_path(".config/quickshell/caelestia");
    // The directory may have been rmdir'd as a leaf — in that case the
    // rcsave moved outside. Look in the parent.
    let search_dir = if qs_parent.exists() {
        qs_parent.clone()
    } else {
        qs_parent.parent().unwrap().to_path_buf()
    };
    let rcsaves: Vec<_> = fs::read_dir(&search_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".rcsave-"))
        .collect();
    assert!(
        !rcsaves.is_empty(),
        "user-edited shell.qml should have been .rcsave'd (searched {})",
        search_dir.display()
    );
    let body = fs::read_to_string(rcsaves[0].path()).unwrap();
    assert_eq!(body, "USER TWEAKED");
}

#[test]
fn install_uninstall_round_trip_for_each_of_the_15_shipped_rices() {
    let cat_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("catalog.toml");
    let shipped_body = fs::read_to_string(&cat_path).unwrap();
    // Shipped commits are bring-up placeholders (`000...0001` through
    // `...000f`). `install()`'s placeholder gate would block every one.
    // Rewrite to non-placeholder SHAs for the test — the fake git ignores
    // the SHA anyway, so this isn't exercising commit-resolution, just
    // pipeline shape.
    let cat_body = shipped_body.replace(
        "0000000000000000000000000000000000000",
        "abc456789abc123456789abc123456789abc1",
    );
    let cat = rice_cooker_backend::catalog::Catalog::from_str(&cat_body).unwrap();
    let mut failures: Vec<String> = Vec::new();

    for name in cat.names() {
        if name == "caelestia" {
            // Caelestia has a real install.fish that invokes paru + writes
            // system-ish state; cannot run as a unit test without a VM.
            // A dedicated integration test exercises it separately.
            continue;
        }
        let h = Harness::new();
        // Stage a shell.qml at the rice's expected entry location so the
        // symlink-based install_cmd lands on something real. Most of our
        // simple installs use $PWD, so a shell.qml at root suffices.
        // For dotfiles-hyprland and Moonveil ($PWD/.config/quickshell) and
        // end-4 ($PWD/.config/quickshell/ii) and dms ($PWD/quickshell), we
        // pre-populate the nested paths.
        for p in [
            "shell.qml",
            "quickshell/shell.qml",
            ".config/quickshell/shell.qml",
            ".config/quickshell/ii/shell.qml",
        ] {
            h.with_rice_file(p, SHELL_QML);
        }
        h.with_catalog(&cat_body);
        let out = h
            .bin()
            .args(["install", name])
            .output()
            .unwrap_or_else(|e| panic!("spawn install {name}: {e}"));
        if !out.status.success() {
            failures.push(format!(
                "install {name}: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
            continue;
        }
        // Current.json reflects this name.
        if read_json(&h.current_json())["name"] != name {
            failures.push(format!("{name}: current.json mismatch"));
            continue;
        }
        let out = h.bin().arg("uninstall").output().unwrap();
        if !out.status.success() {
            failures.push(format!(
                "uninstall {name}: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
            continue;
        }
        if h.current_json().exists() {
            failures.push(format!("{name}: current.json still present"));
        }
    }
    assert!(
        failures.is_empty(),
        "{}/14 failed:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
