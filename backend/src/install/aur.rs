//! AUR integration — clone PKGBUILD, parse makedepends, run makepkg.
//!
//! No yay/paru runtime dep. We clone each AUR package's git repo
//! directly, check out the pinned commit, parse `.SRCINFO` (falling back
//! to `makepkg --printsrcinfo`), run `makepkg --noconfirm --clean
//! --nodeps` as the user, and return the paths of the produced
//! `.pkg.tar.zst` files for the helper to install via pacman -U.
//!
//! Make-time deps are NOT resolved here — they must be pre-installed by
//! the caller (via `helper::install_repo_packages`) so makepkg can run
//! with --nodeps and never need sudo.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow};

/// Clone `https://aur.archlinux.org/<pkg>.git` into `dest` and check out
/// `commit`. `dest` is deleted and re-cloned if it exists (we may be
/// retrying after a crashed prior attempt).
pub fn clone_pkgbuild(pkg: &str, commit: &str, dest: &Path) -> Result<()> {
    if dest.exists() {
        // makepkg leaves a `pkg/` staging dir with 0111 perms — no read,
        // execute only — which breaks `fs::remove_dir_all` (readdir needs
        // read). Shell out to `rm -rf` which handles this by chmodding.
        let status = Command::new("rm")
            .arg("-rf")
            .arg(dest)
            .status()
            .with_context(|| format!("rm -rf {}", dest.display()))?;
        if !status.success() {
            return Err(anyhow!(
                "rm -rf {} exited {:?}",
                dest.display(),
                status.code()
            ));
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let url = format!("https://aur.archlinux.org/{pkg}.git");
    let status = Command::new("git")
        .args(["clone", "--quiet", &url])
        .arg(dest)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("git clone {url}"))?;
    if !status.success() {
        return Err(anyhow!(
            "git clone {url} -> {} exited {:?}",
            dest.display(),
            status.code()
        ));
    }
    let status = Command::new("git")
        .args([
            "-C",
            &dest.to_string_lossy(),
            "checkout",
            "--detach",
            commit,
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .with_context(|| format!("git checkout {commit}"))?;
    if !status.success() {
        return Err(anyhow!(
            "git checkout {commit} in {} exited {:?}",
            dest.display(),
            status.code()
        ));
    }
    Ok(())
}

/// Return the build-time dep set for a cloned PKGBUILD: `depends +
/// makedepends + checkdepends`. Prefers an existing `.SRCINFO`; falls
/// back to `makepkg --printsrcinfo`.
///
/// Why `depends` too: many Python packages list runtime deps (like
/// `pybind11`) that setuptools actually imports at build time to
/// generate wheels. `makepkg --nodeps` skips checking both lists, so
/// we need to pre-install both to avoid ModuleNotFoundError in build().
/// Declared AUR deps (listed as `depends`) are intentionally NOT
/// included here — the caller resolves those separately via `aur_deps`.
pub fn build_time_deps(pkgbuild_dir: &Path, declared_aur: &[String]) -> Result<Vec<String>> {
    let srcinfo = pkgbuild_dir.join(".SRCINFO");
    let body = if srcinfo.exists() {
        fs::read_to_string(&srcinfo).with_context(|| format!("reading {}", srcinfo.display()))?
    } else {
        let out = Command::new("makepkg")
            .args(["--printsrcinfo"])
            .current_dir(pkgbuild_dir)
            .stderr(Stdio::inherit())
            .output()
            .context("running makepkg --printsrcinfo")?;
        if !out.status.success() {
            return Err(anyhow!(
                "makepkg --printsrcinfo in {} exited {:?}",
                pkgbuild_dir.display(),
                out.status.code()
            ));
        }
        String::from_utf8(out.stdout).context("makepkg --printsrcinfo output not UTF-8")?
    };
    let raw = parse_dep_keys(&body, &["depends", "makedepends", "checkdepends"]);
    // Strip any deps the catalog already provides via aur_deps — those
    // are built + installed in order, not pulled from the Arch repos.
    // Also strip the version constraint suffix ("foo>=1.0" → "foo").
    let declared: std::collections::HashSet<&str> =
        declared_aur.iter().map(String::as_str).collect();
    Ok(raw
        .into_iter()
        .map(|d| strip_version_constraint(&d).to_string())
        .filter(|d| !declared.contains(d.as_str()))
        .collect())
}

fn parse_dep_keys(srcinfo: &str, keys: &[&str]) -> Vec<String> {
    // `.SRCINFO` lines look like:
    //     key = value
    // indented with a leading tab. Each key may appear multiple times.
    srcinfo
        .lines()
        .filter_map(|line| {
            let l = line.trim();
            let (k, v) = l.split_once('=')?;
            let k = k.trim();
            if keys.iter().any(|want| *want == k) {
                Some(v.trim().to_string())
            } else {
                None
            }
        })
        .collect()
}

fn strip_version_constraint(dep: &str) -> &str {
    // "foo>=1.0" / "foo=1.0" / "foo<2" / "foo: description" → "foo".
    dep.split(|c: char| matches!(c, '=' | '<' | '>' | ':'))
        .next()
        .unwrap_or(dep)
        .trim()
}

/// Run `makepkg --noconfirm --clean --nodeps` in `pkgbuild_dir` and
/// return the produced `.pkg.tar.zst` (or `.pkg.tar.xz`) paths.
/// `--nodeps` is required: the caller has pre-installed makedepends via
/// the helper, and `--syncdeps` would invoke sudo/pkexec internally,
/// violating our "one pkexec per polkit prompt window" invariant.
pub fn build(pkgbuild_dir: &Path) -> Result<Vec<PathBuf>> {
    let status = Command::new("makepkg")
        .args(["--noconfirm", "--clean", "--nodeps"])
        .current_dir(pkgbuild_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("running makepkg")?;
    if !status.success() {
        return Err(anyhow!(
            "makepkg in {} exited {:?}",
            pkgbuild_dir.display(),
            status.code()
        ));
    }
    // makepkg writes pkgs to the clone dir by default.
    let mut pkgs: Vec<PathBuf> = Vec::new();
    for entry in
        fs::read_dir(pkgbuild_dir).with_context(|| format!("reading {}", pkgbuild_dir.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.ends_with(".pkg.tar.zst") || name.ends_with(".pkg.tar.xz") {
            pkgs.push(entry.path());
        }
    }
    if pkgs.is_empty() {
        return Err(anyhow!(
            "makepkg completed but produced no .pkg.tar.{{zst,xz}} in {}",
            pkgbuild_dir.display()
        ));
    }
    pkgs.sort();
    Ok(pkgs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_depends_makedepends_checkdepends() {
        let srcinfo = r#"
pkgbase = foo
	pkgname = foo
	pkgver = 1.0
	depends = glibc
	depends = pybind11
	makedepends = cmake
	makedepends = ninja
	checkdepends = python-pytest
	optdepends = foo: something
"#;
        let deps = parse_dep_keys(srcinfo, &["depends", "makedepends", "checkdepends"]);
        assert_eq!(deps, vec!["glibc", "pybind11", "cmake", "ninja", "python-pytest"]);
    }

    #[test]
    fn parse_dep_keys_skips_other_keys() {
        let srcinfo = r#"
	pkgver = 1.0
	optdepends = foo
	provides = bar
"#;
        let deps = parse_dep_keys(srcinfo, &["depends", "makedepends"]);
        assert!(deps.is_empty(), "expected empty: {deps:?}");
    }

    #[test]
    fn strips_version_constraints() {
        assert_eq!(strip_version_constraint("foo"), "foo");
        assert_eq!(strip_version_constraint("foo>=1.0"), "foo");
        assert_eq!(strip_version_constraint("foo=1.0"), "foo");
        assert_eq!(strip_version_constraint("foo<2"), "foo");
        assert_eq!(strip_version_constraint("foo: description"), "foo");
    }

    #[test]
    fn build_time_deps_strips_declared_aur_members() {
        // Simulate parse_dep_keys output + strip_version. caelestia-cli
        // is declared as an aur_dep and must not bleed into the pacman-
        // install set (it's built + installed separately).
        let declared = vec!["caelestia-cli".to_string()];
        let raw: Vec<String> = ["caelestia-cli", "glibc", "python"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let filtered: Vec<_> = raw
            .into_iter()
            .filter(|d| !declared.iter().any(|x| x == d))
            .collect();
        assert_eq!(filtered, vec!["glibc", "python"]);
    }
}
