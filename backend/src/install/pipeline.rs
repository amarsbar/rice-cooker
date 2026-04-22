//! install / uninstall / switch / list / status — symlink-only pipeline.
//!
//! Install = clone rice at pinned commit → paru installs deps via pkexec
//! → ln -sfnT into the clone → write install record. Uninstall = remove
//! deps → rcsave the whole clone → rm symlink → rm clone → retire record.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, anyhow};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::catalog::{Catalog, is_placeholder_commit};
use crate::deps;
use crate::git;
use crate::lock::ApplyLock;

use super::env::{Dirs, expand_home};
use super::record::{
    InstallRecord, PacmanDiff, SCHEMA_VERSION, clear_current, load_record, read_current,
    retire_to_previous, save_record, write_current,
};
use super::symlink as symlink_shape;

#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    pub dry_run: bool,
    pub force: bool,
    pub verbose: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallOutcome {
    pub name: String,
    pub pacman_diff: PacmanDiff,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UninstallOutcome {
    pub name: String,
    pub rcsave_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchOutcome {
    pub from: String,
    pub to: String,
    pub rcsave_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListRow {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub installed: bool,
    pub documented_system_effects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatusRow {
    pub installed: Option<InstallRecord>,
}

pub fn install(cat: &Catalog, dirs: &Dirs, name: &str, flags: Flags) -> Result<InstallOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    install_locked(cat, dirs, name, flags)
}

fn install_locked(cat: &Catalog, dirs: &Dirs, name: &str, flags: Flags) -> Result<InstallOutcome> {
    let entry = cat
        .get(name)
        .ok_or_else(|| anyhow!("{name}: not in catalog"))?;

    if is_placeholder_commit(&entry.commit) {
        return Err(anyhow!(
            "{name}: catalog commit is a placeholder ({}). Pin a real SHA in catalog.toml before installing.",
            entry.commit
        ));
    }

    if let Some(cur) = read_current(dirs)? {
        return Err(anyhow!(
            "{cur} is already installed — run uninstall or switch first"
        ));
    }

    if flags.dry_run {
        let dst = expand_home(&entry.symlink_dst, &dirs.home);
        let src = dirs.clone_dir(name).join(&entry.symlink_src);
        println!("would symlink: {} -> {}", dst.display(), src.display());
        let missing_deps =
            deps::missing(&[entry.pacman_deps.clone(), entry.aur_deps.clone()].concat());
        if !missing_deps.is_empty() {
            println!("would install deps: {}", missing_deps.join(" "));
        } else {
            println!("deps already satisfied, zero polkit prompts");
        }
        return Ok(InstallOutcome {
            name: name.to_string(),
            pacman_diff: PacmanDiff::default(),
            dry_run: true,
        });
    }

    // Clone / re-clone.
    let clone = dirs.clone_dir(name);
    if clone.exists() {
        remove_dir_all_forceful(&clone)
            .with_context(|| format!("removing stale clone {}", clone.display()))?;
    }
    log_verbose(flags, &format!("cloning {} @ {}", entry.repo, entry.commit));
    git::clone_at_commit(&entry.repo, &entry.commit, &clone)?;

    // Install deps. Skip paru entirely if nothing's missing.
    let all_deps: Vec<String> = [entry.pacman_deps.clone(), entry.aur_deps.clone()].concat();
    let missing_deps = deps::missing(&all_deps);
    if !missing_deps.is_empty() {
        log_verbose(flags, &format!("install deps: {}", missing_deps.join(" ")));
        deps::install_packages(&missing_deps)?;
    } else if !all_deps.is_empty() {
        log_verbose(flags, "deps already satisfied");
    }

    // pacman -Qqe diff captures whatever paru pulled (including transitive).
    let pre_explicit = pacman_explicit();
    let _ = pre_explicit; // placeholder; see below

    // Create symlink.
    let paths = symlink_shape::create_symlink(&clone, entry, &dirs.home)?;

    let post_explicit = pacman_explicit();
    let added_explicit = diff_explicit(&pre_explicit, &post_explicit);

    // Write record.
    let record = InstallRecord {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        commit: entry.commit.clone(),
        installed_at: InstallRecord::now_rfc3339(),
        symlink_path: paths.symlink_path.clone(),
        symlink_target: paths.symlink_target.clone(),
        pacman_diff: PacmanDiff {
            added_explicit: added_explicit.clone(),
        },
    };
    save_record(&dirs.record_json(name), &record)?;
    write_current(dirs, name)?;

    Ok(InstallOutcome {
        name: name.to_string(),
        pacman_diff: PacmanDiff { added_explicit },
        dry_run: false,
    })
}

pub fn uninstall(dirs: &Dirs, flags: Flags) -> Result<UninstallOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    uninstall_locked(dirs, flags)
}

fn uninstall_locked(dirs: &Dirs, flags: Flags) -> Result<UninstallOutcome> {
    let name = read_current(dirs)?.ok_or_else(|| anyhow!("no rice installed"))?;
    let record = load_record(&dirs.record_json(&name))?;

    if flags.dry_run {
        println!("would remove symlink {}", record.symlink_path.display());
        println!(
            "would remove packages: {}",
            record.pacman_diff.added_explicit.join(" ")
        );
        return Ok(UninstallOutcome {
            name,
            rcsave_dir: None,
        });
    }

    // Remove packages. Pre-filter already-removed so pacman doesn't
    // abort with "target not found" on retry.
    if !record.pacman_diff.added_explicit.is_empty() {
        let still_installed = deps::installed(&record.pacman_diff.added_explicit);
        if !still_installed.is_empty() {
            log_verbose(
                flags,
                &format!("remove packages: {}", still_installed.join(" ")),
            );
            deps::remove_packages(&still_installed)?;
        }
    }

    // Save user edits. Whole-clone copy, unconditional. Timestamp +
    // PID avoids collisions on rapid switch cycles.
    let rcsave = dirs.cache.join("rcsave").join(format!(
        "{name}-{}-{}",
        now_ts_compact(),
        std::process::id(),
    ));
    let clone = dirs.clone_dir(&name);
    let rcsave_dir = if clone.exists() {
        fs::create_dir_all(&rcsave).with_context(|| format!("creating {}", rcsave.display()))?;
        let status = Command::new("cp")
            .args(["-rT"])
            .arg(&clone)
            .arg(&rcsave)
            .status()
            .context("cp -rT")?;
        if !status.success() {
            return Err(anyhow!(
                "cp -rT {} {} exited {:?}",
                clone.display(),
                rcsave.display(),
                status.code()
            ));
        }
        Some(rcsave)
    } else {
        None
    };

    // Remove symlink, clone, retire record.
    let _ = fs::remove_file(&record.symlink_path);
    let _ = remove_dir_all_forceful(&clone);
    retire_to_previous(dirs, &name)?;
    clear_current(dirs)?;

    Ok(UninstallOutcome { name, rcsave_dir })
}

pub fn switch(cat: &Catalog, dirs: &Dirs, to: &str, flags: Flags) -> Result<SwitchOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;
    let uo = uninstall_locked(dirs, flags)?;
    let io = install_locked(cat, dirs, to, flags)?;
    Ok(SwitchOutcome {
        from: uo.name,
        to: io.name,
        rcsave_dir: uo.rcsave_dir,
    })
}

pub fn list(cat: &Catalog, dirs: &Dirs) -> Result<Vec<ListRow>> {
    let current = read_current(dirs)?;
    let mut rows = Vec::new();
    for (name, entry) in &cat.rices {
        rows.push(ListRow {
            name: name.clone(),
            display_name: entry.display_name.clone(),
            description: entry.description.clone(),
            installed: current.as_deref() == Some(name.as_str()),
            documented_system_effects: entry.documented_system_effects.clone(),
        });
    }
    Ok(rows)
}

pub fn status(dirs: &Dirs) -> Result<StatusRow> {
    let name = match read_current(dirs)? {
        Some(n) => n,
        None => return Ok(StatusRow { installed: None }),
    };
    let record = load_record(&dirs.record_json(&name))?;
    Ok(StatusRow {
        installed: Some(record),
    })
}

fn pacman_explicit() -> Vec<String> {
    let out = match Command::new("pacman").args(["-Qqe"]).output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

fn diff_explicit(pre: &[String], post: &[String]) -> Vec<String> {
    use std::collections::HashSet;
    let pre_set: HashSet<&str> = pre.iter().map(String::as_str).collect();
    let mut added: Vec<String> = post
        .iter()
        .filter(|p| !pre_set.contains(p.as_str()))
        .cloned()
        .collect();
    added.sort();
    added
}

/// Shell out to `rm -rf` for fs::remove_dir_all-resistant cases
/// (makepkg's pkg/ dir with 0111 perms, stray immutable files).
fn remove_dir_all_forceful(path: &std::path::Path) -> Result<()> {
    let status = Command::new("rm")
        .arg("-rf")
        .arg(path)
        .status()
        .with_context(|| format!("rm -rf {}", path.display()))?;
    if !status.success() {
        return Err(anyhow!(
            "rm -rf {} exited {:?}",
            path.display(),
            status.code()
        ));
    }
    Ok(())
}

fn now_ts_compact() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "unknown".into())
        .replace(':', "")
}

fn log_verbose(flags: Flags, msg: &str) {
    if flags.verbose {
        eprintln!("rice-cooker: {msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_explicit_finds_added() {
        let pre = vec!["a".into(), "b".into(), "c".into()];
        let post = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
        assert_eq!(diff_explicit(&pre, &post), vec!["d", "e"]);
    }

    #[test]
    fn diff_explicit_ignores_removed() {
        let pre = vec!["a".into(), "b".into()];
        let post = vec!["a".into()];
        assert!(diff_explicit(&pre, &post).is_empty());
    }

    #[test]
    fn diff_explicit_empty_when_unchanged() {
        let pre = vec!["a".into(), "b".into()];
        let post = vec!["a".into(), "b".into()];
        assert!(diff_explicit(&pre, &post).is_empty());
    }
}
