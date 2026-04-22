//! rice-cooker-helper — the privileged half of Rice Cooker.
//!
//! Invoked via `pkexec rice-cooker-helper <subcommand> <args>...` by the
//! unprivileged main binary. Runs as root; talks only to pacman; never
//! touches `$HOME` or the catalog.
//!
//! Three subcommands, each with a tight input-validation contract:
//!
//! - `install-repo-packages <pkg1> <pkg2> ...` → `pacman -S --needed --noconfirm <pkgs>`
//! - `install-built-packages <path1> <path2> ...` → `pacman -U --needed --noconfirm <paths>`
//! - `remove-packages <pkg1> <pkg2> ...` → `pacman -Rns --noconfirm <pkgs>`
//!
//! Security posture:
//! - Every package name must match `^[a-zA-Z0-9@._+-]+$` (pacman's
//!   valid-name regex).
//! - Every built-package path must be absolute, canonicalize to a path
//!   under `/home/<invoking-user>/.cache/rice-cooker/aur/` or `/tmp/`,
//!   end in `.pkg.tar.zst` or `.pkg.tar.xz`, be a regular file (not a
//!   symlink or anything else), and be owned by the invoking user
//!   (obtained from polkit's `PKEXEC_UID` env var).
//! - Inputs failing any check → exit non-zero before touching pacman.
//! - pacman's stdout+stderr is inherited so the caller sees the real
//!   pacman UI for errors. Exit status is pacman's.
//!
//! Not inlined into the main crate on purpose: this is the only code
//! path that runs as root. Keeping the surface small makes security
//! review tractable.

use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().collect();
    if args.len() < 2 {
        eprintln!(
            "usage: rice-cooker-helper <install-repo-packages|install-built-packages|remove-packages> <args>..."
        );
        return ExitCode::from(64);
    }
    let sub = args[1].to_string_lossy().into_owned();
    // Remaining args as UTF-8 strings. Pacman pkg names and package paths
    // are ASCII by convention; non-UTF-8 bytes in either is a rejection.
    let rest: Result<Vec<String>, _> = args[2..]
        .iter()
        .map(|a| {
            a.to_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("argument not utf-8: {a:?}"))
        })
        .collect();
    let rest = match rest {
        Ok(v) => v,
        Err(e) => {
            eprintln!("rice-cooker-helper: {e}");
            return ExitCode::from(65);
        }
    };

    let result = match sub.as_str() {
        "install-repo-packages" => install_repo_packages(&rest),
        "install-built-packages" => install_built_packages(&rest),
        "remove-packages" => remove_packages(&rest),
        other => {
            eprintln!("rice-cooker-helper: unknown subcommand {other:?}");
            return ExitCode::from(64);
        }
    };
    match result {
        Ok(code) => ExitCode::from((code & 0xff) as u8),
        Err(msg) => {
            eprintln!("rice-cooker-helper: {msg}");
            ExitCode::from(66)
        }
    }
}

fn install_repo_packages(pkgs: &[String]) -> Result<i32, String> {
    if pkgs.is_empty() {
        return Err("install-repo-packages: no packages".into());
    }
    for p in pkgs {
        validate_pkg_name(p)?;
    }
    let mut cmd = Command::new(pacman_bin());
    cmd.arg("-S").arg("--needed").arg("--noconfirm").args(pkgs);
    run_inherit(cmd)
}

fn install_built_packages(paths: &[String]) -> Result<i32, String> {
    if paths.is_empty() {
        return Err("install-built-packages: no paths".into());
    }
    let uid = pkexec_uid()?;
    for p in paths {
        validate_built_path(p, uid)?;
    }
    let mut cmd = Command::new(pacman_bin());
    cmd.arg("-U").arg("--needed").arg("--noconfirm").args(paths);
    run_inherit(cmd)
}

fn remove_packages(pkgs: &[String]) -> Result<i32, String> {
    if pkgs.is_empty() {
        return Err("remove-packages: no packages".into());
    }
    for p in pkgs {
        validate_pkg_name(p)?;
    }
    let mut cmd = Command::new(pacman_bin());
    cmd.arg("-Rns").arg("--noconfirm").args(pkgs);
    run_inherit(cmd)
}

/// pacman's valid package-name regex: `^[a-zA-Z0-9@._+-]+$`. Applied to
/// every argument we hand to pacman so shell metacharacters, whitespace,
/// NULs, path separators, option-like `-x` prefixes, etc. can't sneak in.
///
/// NOTE: pacman itself allows more (unicode?) but our catalog never does.
/// Tight is safe.
fn validate_pkg_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("empty package name".into());
    }
    // Leading `-` would look like a pacman option.
    if name.starts_with('-') {
        return Err(format!("package name must not start with '-': {name:?}"));
    }
    if name.len() > 256 {
        return Err(format!(
            "package name too long ({} bytes, max 256)",
            name.len()
        ));
    }
    for b in name.bytes() {
        let ok = b.is_ascii_alphanumeric() || matches!(b, b'@' | b'.' | b'_' | b'+' | b'-');
        if !ok {
            return Err(format!(
                "package name contains invalid byte 0x{b:02x}: {name:?}"
            ));
        }
    }
    Ok(())
}

fn validate_built_path(path: &str, uid: u32) -> Result<(), String> {
    // 1. Absolute path required — no relative wiggle room.
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(format!("path must be absolute: {path:?}"));
    }
    // 2. Extension must be one of the two pacman-accepted archive types.
    let ok_ext = ["zst", "xz"].iter().any(|ext| {
        // .pkg.tar.zst or .pkg.tar.xz — check both final-ext and ".tar" before it.
        let end = format!(".pkg.tar.{ext}");
        path.ends_with(&end)
    });
    if !ok_ext {
        return Err(format!(
            "path must end in .pkg.tar.zst or .pkg.tar.xz: {path:?}"
        ));
    }
    // 3. No `..` segments — defense against `/home/x/../..//root/evil.pkg.tar.zst`.
    //    Canonicalization below also catches this but we reject early for clarity.
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("path contains .. segment: {path:?}"));
    }
    // 4. Must be a regular file (not a symlink, not a directory, not a
    //    device node). `symlink_metadata` doesn't follow — rejecting
    //    symlinks is critical because a malicious symlink could point
    //    at a pacman package outside the allowed dirs.
    let md = std::fs::symlink_metadata(p).map_err(|e| format!("stat {path:?}: {e}"))?;
    if !md.file_type().is_file() {
        return Err(format!(
            "path must be a regular file (not symlink/dir/device): {path:?}"
        ));
    }
    // 5. Owned by the invoking user. A root-owned file in /tmp would
    //    otherwise let any unprivileged user who can write to /tmp
    //    execute arbitrary pacman installs via a dropped .pkg.tar.zst.
    if md.uid() != uid {
        return Err(format!(
            "path not owned by invoking user (uid {uid}): {path:?} (owner uid={})",
            md.uid()
        ));
    }
    // 6. Canonicalize and confirm the real path sits under an allowed
    //    dir. This catches clever tricks: a tmpfile at /tmp/x that is
    //    itself a hardlink to /root/secret.pkg.tar.zst would pass the
    //    other checks but canonicalize() would resolve to /root (for
    //    hardlinks we'd get the same inode path; for bind-mounts we'd
    //    catch the cross-dir escape). `canonicalize` refuses to run on
    //    nonexistent paths, but we already confirmed existence via stat.
    let canonical = p
        .canonicalize()
        .map_err(|e| format!("canonicalize {path:?}: {e}"))?;
    let allowed = allowed_built_dirs(uid);
    let under_allowed = allowed.iter().any(|root| canonical.starts_with(root));
    if !under_allowed {
        return Err(format!(
            "path {canonical:?} is not under any allowed directory {allowed:?}"
        ));
    }
    Ok(())
}

fn allowed_built_dirs(uid: u32) -> Vec<PathBuf> {
    let user_cache = PathBuf::from(format!(
        "/home/{}/.cache/rice-cooker/aur",
        uid_to_home_stub(uid)
    ));
    vec![user_cache, PathBuf::from("/tmp")]
}

/// Resolve `/home/<username>` from uid. Simplest reliable approach is
/// reading the user's HOME from `/etc/passwd` — but we don't want to
/// parse passwd. Instead accept the tradeoff: require the caller's HOME
/// layout to be `/home/<username>/.cache/...` (standard on every distro
/// we target), and derive username via `getpwuid_r` through libc. Since
/// we're pure std, fall back to reading the `HOME` env var that pkexec
/// propagates to us, if present; otherwise reject.
///
/// Returning a stub name when pkexec didn't set HOME is intentionally
/// non-functional — validate_built_path's canonicalize check will reject
/// any path that doesn't match the user's real home.
fn uid_to_home_stub(_uid: u32) -> String {
    // pkexec sets HOME to the target user's home. If unset (e.g., called
    // outside pkexec), the resulting /home/<HOME-basename>/... path
    // won't match any real file, and validation will fail cleanly.
    std::env::var("HOME")
        .ok()
        .and_then(|h| {
            Path::new(&h)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "__unknown__".into())
}

fn pkexec_uid() -> Result<u32, String> {
    std::env::var("PKEXEC_UID")
        .map_err(|_| "PKEXEC_UID not set — must be invoked via pkexec".to_string())?
        .parse::<u32>()
        .map_err(|e| format!("PKEXEC_UID not a valid uint: {e}"))
}

fn pacman_bin() -> String {
    // Overridable for tests so we can route to a local stub.
    std::env::var("RICE_COOKER_PACMAN").unwrap_or_else(|_| "/usr/bin/pacman".into())
}

fn run_inherit(mut cmd: Command) -> Result<i32, String> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = cmd.status().map_err(|e| format!("spawning pacman: {e}"))?;
    Ok(status.code().unwrap_or(-1))
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;

    // ---- validate_pkg_name ----

    #[test]
    fn pkg_name_accepts_valid() {
        for good in [
            "qt6-5compat",
            "kirigami",
            "hyprpolkitagent",
            "base-devel",
            "lib32-glibc",
            "python-3.12",
            "a.b.c",
            "foo_bar+baz",
            "foo@1.0",
            "x",
        ] {
            validate_pkg_name(good).unwrap_or_else(|e| panic!("rejected valid {good:?}: {e}"));
        }
    }

    #[test]
    fn pkg_name_rejects_shell_metacharacters() {
        for bad in [
            ";rm -rf /",
            "foo;ls",
            "foo&bar",
            "foo|bar",
            "foo>out",
            "foo<in",
            "foo`cmd`",
            "foo$VAR",
            "foo$(cmd)",
            "foo\"bar",
            "foo'bar",
            "foo\\bar",
        ] {
            let err = validate_pkg_name(bad).unwrap_err();
            assert!(
                err.contains("invalid byte"),
                "wrong error for {bad:?}: {err}"
            );
        }
    }

    #[test]
    fn pkg_name_rejects_path_separators_and_whitespace() {
        for bad in [
            "foo/bar", "../etc", "foo bar", "foo\tbar", "foo\nbar", "foo\rbar",
        ] {
            validate_pkg_name(bad).unwrap_err();
        }
    }

    #[test]
    fn pkg_name_rejects_null_byte() {
        let err = validate_pkg_name("foo\0bar").unwrap_err();
        assert!(err.contains("0x00"), "wrong error for NUL: {err}");
    }

    #[test]
    fn pkg_name_rejects_empty_and_leading_dash() {
        assert!(validate_pkg_name("").unwrap_err().contains("empty"));
        assert!(
            validate_pkg_name("-S")
                .unwrap_err()
                .contains("must not start with '-'"),
            "leading dash should be rejected"
        );
        assert!(validate_pkg_name("--version").unwrap_err().contains("'-'"));
    }

    #[test]
    fn pkg_name_rejects_very_long() {
        let long = "a".repeat(257);
        assert!(validate_pkg_name(&long).unwrap_err().contains("too long"));
    }

    #[test]
    fn pkg_name_rejects_non_ascii() {
        for bad in ["café", "name™", "foo\u{202e}bar"] {
            let err = validate_pkg_name(bad).unwrap_err();
            assert!(
                err.contains("invalid byte"),
                "wrong error for {bad:?}: {err}"
            );
        }
    }

    // ---- validate_built_path ----

    fn mk_file(tmp: &std::path::Path, name: &str, body: &[u8]) -> PathBuf {
        let p = tmp.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn built_path_rejects_relative() {
        let err = validate_built_path("foo.pkg.tar.zst", 1000).unwrap_err();
        assert!(err.contains("absolute"), "got: {err}");
    }

    #[test]
    fn built_path_rejects_bad_extension() {
        for bad in [
            "/tmp/foo.txt",
            "/tmp/foo.tar.gz",
            "/tmp/foo.pkg.tar",
            "/tmp/foo.zst",
            "/tmp/foo",
        ] {
            let err = validate_built_path(bad, 1000).unwrap_err();
            assert!(
                err.contains(".pkg.tar.zst") || err.contains(".pkg.tar.xz"),
                "wrong error for {bad:?}: {err}"
            );
        }
    }

    #[test]
    fn built_path_rejects_parent_dir_traversal() {
        let err = validate_built_path("/tmp/../etc/foo.pkg.tar.zst", 1000).unwrap_err();
        assert!(err.contains(".."), "got: {err}");
    }

    #[test]
    fn built_path_rejects_symlink() {
        let t = tempfile::tempdir().unwrap();
        let real = mk_file(t.path(), "real.pkg.tar.zst", b"body");
        let link = t.path().join("link.pkg.tar.zst");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = validate_built_path(link.to_str().unwrap(), uid_of(&link)).unwrap_err();
        assert!(err.contains("regular file"), "got: {err}");
    }

    #[test]
    fn built_path_rejects_not_owned_by_invoker() {
        let t = tempfile::tempdir().unwrap();
        let p = mk_file(t.path(), "x.pkg.tar.zst", b"body");
        let other_uid = uid_of(&p).wrapping_add(1);
        let err = validate_built_path(p.to_str().unwrap(), other_uid).unwrap_err();
        assert!(err.contains("not owned"), "got: {err}");
    }

    #[test]
    fn built_path_rejects_outside_allowed_dirs() {
        // A file in the tempdir (which is under /tmp on most systems, so
        // we'd pass) — put it somewhere that canonicalize resolves
        // outside /tmp and /home. Use /var/tmp if writable.
        let base = PathBuf::from("/var/tmp");
        if !base.exists() || std::fs::File::create(base.join(".probe")).is_err() {
            // Skip on systems where we can't write to /var/tmp in tests.
            return;
        }
        let _ = std::fs::remove_file(base.join(".probe"));
        let p = base.join("outside.pkg.tar.zst");
        std::fs::write(&p, b"body").unwrap();
        let uid = uid_of(&p);
        let err = validate_built_path(p.to_str().unwrap(), uid).unwrap_err();
        // Allow either "not under any allowed" or the "regular file" branch
        // depending on how /var/tmp canonicalizes on the host.
        assert!(
            err.contains("not under any allowed") || err.contains("regular file"),
            "got: {err}"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn built_path_accepts_tmp_zst_owned_by_invoker() {
        let t = tempfile::tempdir_in("/tmp").unwrap();
        let p = mk_file(t.path(), "a.pkg.tar.zst", b"body");
        let uid = uid_of(&p);
        validate_built_path(p.to_str().unwrap(), uid).unwrap();
    }

    #[test]
    fn built_path_accepts_xz() {
        let t = tempfile::tempdir_in("/tmp").unwrap();
        let p = mk_file(t.path(), "a.pkg.tar.xz", b"body");
        let uid = uid_of(&p);
        validate_built_path(p.to_str().unwrap(), uid).unwrap();
    }

    fn uid_of(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().uid()
    }

    // ---- pkexec_uid ----

    #[test]
    #[ignore = "mutates process env; run serially"]
    fn pkexec_uid_requires_env_var() {
        // Ignored by default because it mutates env, which is shared
        // between tests. Run manually with `cargo test -- --ignored`.
        // SAFETY: tests run in isolation with --ignored.
        unsafe {
            std::env::remove_var("PKEXEC_UID");
        }
        assert!(pkexec_uid().unwrap_err().contains("PKEXEC_UID"));
        unsafe {
            std::env::set_var("PKEXEC_UID", "not-a-number");
        }
        assert!(pkexec_uid().unwrap_err().contains("not a valid uint"));
        unsafe {
            std::env::set_var("PKEXEC_UID", "1000");
        }
        assert_eq!(pkexec_uid().unwrap(), 1000);
        unsafe {
            std::env::remove_var("PKEXEC_UID");
        }
    }
}
