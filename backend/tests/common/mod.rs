//! Sandboxed harness for install/uninstall integration tests.
//!
//! Each Harness instance carves out a fresh tempdir + fake HOME, populates
//! $PATH with fake git (so clone_at_commit copies fixture content), and
//! lets the test skip pacman/systemd interaction via env flags and the
//! `--skip-pacman` CLI flag.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use tempfile::TempDir;

/// Fake git that `clone --no-checkout && checkout --detach <sha>` into a
/// dest by copying from `$FAKE_RICE_SOURCE` into the dest. Ignores the
/// commit SHA since we're just shoveling fixture content.
pub const FAKE_GIT: &str = r#"#!/bin/sh
# strip top-level git options
while [ $# -gt 0 ]; do
    case "$1" in
        --version) echo "git version fake 2.0.0"; exit 0 ;;
        -c) shift 2 ;;
        -C) shift 2 ;;
        *) break ;;
    esac
done
cmd="$1"; shift
case "$cmd" in
    clone)
        DEST=""
        while [ $# -gt 0 ]; do
            case "$1" in
                --) shift ;;
                -*) shift ;;
                *)
                    if [ -z "$_URL" ]; then _URL="$1"; shift
                    else DEST="$1"; shift
                    fi
                    ;;
            esac
        done
        if [ -z "$DEST" ]; then exit 1; fi
        mkdir -p "$DEST/.git"
        if [ -n "$FAKE_RICE_SOURCE" ] && [ -d "$FAKE_RICE_SOURCE" ]; then
            cp -r "$FAKE_RICE_SOURCE"/. "$DEST/"
        fi
        exit 0
        ;;
    checkout|fetch|reset|init|config|commit|add)
        exit 0
        ;;
esac
exit 0
"#;

pub struct Harness {
    // Kept alive so the tempdir isn't dropped mid-test.
    #[allow(dead_code)]
    pub tmp: TempDir,
    pub fakes_dir: PathBuf,
    pub home: PathBuf,
    pub cache_dir: PathBuf,
    pub data_dir: PathBuf,
    pub rice_source: PathBuf,
    pub catalog: PathBuf,
}

impl Harness {
    pub fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let fakes_dir = tmp.path().join("fakes");
        let home = tmp.path().join("home");
        let cache_dir = home.join(".cache");
        let data_dir = home.join(".local/share");
        let rice_source = tmp.path().join("rice-src");
        let catalog = tmp.path().join("catalog.toml");
        for d in [&fakes_dir, &home, &cache_dir, &data_dir, &rice_source] {
            fs::create_dir_all(d).unwrap();
        }
        // Install fake git.
        let git = fakes_dir.join("git");
        fs::write(&git, FAKE_GIT).unwrap();
        let mut perms = fs::metadata(&git).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&git, perms).unwrap();
        Self {
            tmp,
            fakes_dir,
            home,
            cache_dir,
            data_dir,
            rice_source,
            catalog,
        }
    }

    /// Place a file inside the fixture rice source.
    pub fn with_rice_file(&self, rel: &str, body: &str) -> &Self {
        let p = self.rice_source.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
        self
    }

    /// Write the catalog file with the given TOML body.
    pub fn with_catalog(&self, body: &str) -> &Self {
        fs::write(&self.catalog, body).unwrap();
        self
    }

    pub fn bin(&self) -> Command {
        let mut cmd = Command::cargo_bin("rice-cooker-backend").unwrap();
        cmd.env_clear();
        // Keep real /usr/bin on PATH for sh/bash/cp/mkdir/ln/tee/etc; prepend
        // fakes so our git wins.
        cmd.env(
            "PATH",
            format!("{}:/usr/bin:/bin", self.fakes_dir.display()),
        );
        cmd.env("HOME", &self.home);
        cmd.env("XDG_CACHE_HOME", &self.cache_dir);
        cmd.env("XDG_DATA_HOME", &self.data_dir);
        cmd.env("FAKE_RICE_SOURCE", &self.rice_source);
        cmd.arg("--catalog").arg(&self.catalog).arg("--skip-pacman");
        cmd
    }

    /// Path to `.local/share/rice-cooker/installs/current.json`.
    pub fn current_json(&self) -> PathBuf {
        self.data_dir.join("rice-cooker/installs/current.json")
    }

    pub fn record_json(&self, name: &str) -> PathBuf {
        self.data_dir
            .join("rice-cooker/installs")
            .join(format!("{name}.json"))
    }

    pub fn previous_json(&self) -> PathBuf {
        self.data_dir.join("rice-cooker/installs/previous.json")
    }

    pub fn home_path(&self, rel: &str) -> PathBuf {
        self.home.join(rel)
    }
}

#[allow(dead_code)]
pub fn read_json(path: &Path) -> serde_json::Value {
    let s = fs::read_to_string(path).unwrap();
    serde_json::from_str(&s).unwrap()
}
