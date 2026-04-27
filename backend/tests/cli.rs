//! End-to-end tests for the `rice-cooker-backend` binary.
//!
//! These exercise the CLI surface (flag shapes, subcommand names, JSON
//! output shapes) the way `main.rs` wires them up — the inline unit
//! tests at module bottoms can't catch a regression at the argv /
//! stdout boundary.
//!
//! Mostly read-only subcommands. `preview` is only exercised on the
//! missing-catalog-entry path because the successful path shells out.

use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

const CATALOG: &str = r#"
[one]
display_name = "One"
description = "first rice"
repo = "https://example.invalid/one"
commit = "0123456789abcdef0123456789abcdef01234567"
symlink_src = "."
symlink_dst = "~/.config/quickshell/one"
install_supported = true
documented_system_effects = ["writes /etc/thing"]

[two]
display_name = "Two"
repo = "https://example.invalid/two"
commit = "fedcba9876543210fedcba9876543210fedcba98"
symlink_src = "."
symlink_dst = "~/.config/quickshell/two"
"#;

fn scratch() -> TempDir {
    let t = TempDir::new().unwrap();
    fs::write(t.path().join("catalog.toml"), CATALOG).unwrap();
    t
}

fn cmd(t: &TempDir) -> Command {
    let mut c = Command::cargo_bin("rice-cooker-backend").unwrap();
    // Isolate from the developer's real rice-cooker state. `Paths::from_env`
    // honors HOME + XDG_* + RICE_COOKER_CACHE_DIR.
    c.env_clear()
        .env("HOME", t.path())
        .env("XDG_CACHE_HOME", t.path().join("cache"))
        .env("XDG_DATA_HOME", t.path().join("data"))
        .env("RICE_COOKER_CACHE_DIR", t.path().join("cache/rice-cooker"))
        .env("PATH", std::env::var("PATH").unwrap_or_default());
    c
}

#[test]
fn list_prints_catalog_entries_as_json_array() {
    let t = scratch();
    let out = cmd(&t)
        .args(["--catalog", t.path().join("catalog.toml").to_str().unwrap()])
        .arg("list")
        .assert()
        .success();
    let stdout = std::str::from_utf8(&out.get_output().stdout).unwrap();
    insta::assert_snapshot!("list_json", stdout);
}

#[test]
fn preview_wires_to_preview_subcommand() {
    let t = scratch();
    let out = cmd(&t)
        .args([
            "--catalog",
            t.path().join("catalog.toml").to_str().unwrap(),
            "preview",
            "missing",
        ])
        .assert()
        .failure();
    let stdout = std::str::from_utf8(&out.get_output().stdout).unwrap();
    assert!(stdout.contains(r#""subcommand":"preview""#));
    assert!(stdout.contains(r#""stage":"preflight""#));
    assert!(stdout.contains("missing: not in catalog"));
}
