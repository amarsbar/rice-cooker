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
        fs::remove_dir_all(dest).with_context(|| format!("removing stale {}", dest.display()))?;
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

/// Return the list of makedepends for a cloned PKGBUILD. Prefers an
/// existing `.SRCINFO`; falls back to running `makepkg --printsrcinfo`
/// inside the clone. Both fail → error, catalog maintainer's problem.
pub fn makedepends(pkgbuild_dir: &Path) -> Result<Vec<String>> {
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
    Ok(parse_makedepends(&body))
}

fn parse_makedepends(srcinfo: &str) -> Vec<String> {
    // `.SRCINFO` lines look like:
    //     key = value
    // indented with a leading tab. makedepends may appear multiple times.
    srcinfo
        .lines()
        .filter_map(|line| {
            let l = line.trim();
            let (k, v) = l.split_once('=')?;
            if k.trim() == "makedepends" {
                Some(v.trim().to_string())
            } else {
                None
            }
        })
        .collect()
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
    fn parse_makedepends_finds_multiple() {
        let srcinfo = r#"
pkgbase = foo
	pkgname = foo
	pkgver = 1.0
	makedepends = cmake
	makedepends = ninja
	makedepends = git
	depends = glibc
"#;
        let deps = parse_makedepends(srcinfo);
        assert_eq!(deps, vec!["cmake", "ninja", "git"]);
    }

    #[test]
    fn parse_makedepends_ignores_other_keys() {
        let srcinfo = r#"
	pkgver = 1.0
	depends = a
	depends = b
	optdepends = c
"#;
        let deps = parse_makedepends(srcinfo);
        assert!(
            deps.is_empty(),
            "should ignore depends/optdepends: {deps:?}"
        );
    }

    #[test]
    fn parse_makedepends_on_empty_srcinfo_is_empty() {
        assert!(parse_makedepends("").is_empty());
    }
}
