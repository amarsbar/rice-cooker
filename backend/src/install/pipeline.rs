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
            deps::missing(&[entry.pacman_deps.clone(), entry.aur_deps.clone()].concat())?;
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
    println!("cloning {} @ {}", entry.repo, entry.commit);
    git::clone_at_commit(&entry.repo, &entry.commit, &clone)?;

    // pacman -Qqe diff captures whatever paru pulled (including
    // transitive AUR deps). MUST happen before install_packages so
    // `post - pre` reflects only this install's additions — capturing
    // pre-state after the install would always yield an empty diff
    // and uninstall would leave packages orphaned.
    let pre_explicit = pacman_explicit()
        .context("pacman -Qqe pre-snapshot (refusing to install without a reliable baseline)")?;

    // Install deps. Skip paru entirely if nothing's missing.
    let all_deps: Vec<String> = [entry.pacman_deps.clone(), entry.aur_deps.clone()].concat();
    let missing_deps = deps::missing(&all_deps)?;
    if !missing_deps.is_empty() {
        println!("install deps: {}", missing_deps.join(" "));
        deps::install_packages(&missing_deps)?;
    } else if !all_deps.is_empty() {
        println!("deps already satisfied");
    }

    let post_explicit = pacman_explicit().context("pacman -Qqe post-snapshot")?;
    let added_explicit = diff_explicit(&pre_explicit, &post_explicit);

    // Persist ownership BEFORE create_symlink so a symlink failure still
    // leaves a record that uninstall can use to remove the just-installed
    // packages. Symlink paths are deterministic from the entry + home,
    // so we can compute them without actually making the symlink yet.
    let symlink_path = expand_home(&entry.symlink_dst, &dirs.home);
    let symlink_target = clone.join(&entry.symlink_src);
    let record = InstallRecord {
        schema_version: SCHEMA_VERSION,
        name: name.to_string(),
        commit: entry.commit.clone(),
        installed_at: InstallRecord::now_rfc3339(),
        symlink_path,
        symlink_target,
        pacman_diff: PacmanDiff {
            added_explicit: added_explicit.clone(),
        },
    };
    save_record(&dirs.record_json(name), &record)?;
    write_current(dirs, name)?;

    // Now create the symlink. If it fails, the record is already on
    // disk so uninstall can roll back the packages — but the user
    // hitting this mid-install has no idea they need to uninstall;
    // `install <name>` would just keep erroring with "already
    // installed". Wrap the error with a remediation hint.
    if let Err(e) = symlink_shape::create_symlink(&clone, entry, &dirs.home) {
        return Err(anyhow!(
            "{e:#}\n\
             install left {} package(s) registered to this rice but no symlink. \
             Run `rice-cooker uninstall` to roll back, then retry install.",
            added_explicit.len()
        ));
    }

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
    // abort with "target not found" on retry. A pacman-query failure
    // here propagates — silently skipping removal would strand the
    // rice's packages on disk with no tool-visible owner.
    if !record.pacman_diff.added_explicit.is_empty() {
        let still_installed = deps::installed(&record.pacman_diff.added_explicit)?;
        if !still_installed.is_empty() {
            println!("remove packages: {}", still_installed.join(" "));
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
        // `-a` preserves mode, ownership, timestamps, symlinks — rice
        // trees commonly ship executable scripts and internal symlinks,
        // and `-r` alone loses both. `-T` prevents the copy-into-target
        // footgun where target gets nested under itself.
        let status = Command::new("cp")
            .args(["-aT"])
            .arg(&clone)
            .arg(&rcsave)
            .status()
            .context("cp -aT")?;
        if !status.success() {
            return Err(anyhow!(
                "cp -aT {} {} exited {:?}",
                clone.display(),
                rcsave.display(),
                status.code()
            ));
        }
        Some(rcsave)
    } else {
        None
    };

    // Remove symlink — but only if it's still OUR symlink. User could
    // have replaced it with a regular file/dir since install; we don't
    // want to clobber that. NotFound is idempotent-OK; type mismatch or
    // target mismatch is skipped with a warning that includes both
    // paths for diagnostics; real IO errors bubble up.
    //
    // If we skip (retargeted/replaced), the record is still retired +
    // current.json cleared below, so the rice is "uninstalled" from the
    // tool's view even though the user-owned file at symlink_path stays.
    match fs::symlink_metadata(&record.symlink_path) {
        Ok(md) if md.file_type().is_symlink() => match fs::read_link(&record.symlink_path) {
            Ok(t) if t == record.symlink_target => {
                fs::remove_file(&record.symlink_path).with_context(|| {
                    format!("removing symlink {}", record.symlink_path.display())
                })?;
            }
            Ok(t) => {
                eprintln!(
                    "rice-cooker: skipping {}: symlink target is {:?}, record says {:?} (user-retargeted?)",
                    record.symlink_path.display(),
                    t,
                    record.symlink_target
                );
            }
            Err(e) => {
                return Err(anyhow!("read_link {}: {e}", record.symlink_path.display()));
            }
        },
        Ok(_) => {
            eprintln!(
                "rice-cooker: skipping {}: not a symlink anymore (user-replaced?)",
                record.symlink_path.display()
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow!("reading {}: {e}", record.symlink_path.display()));
        }
    }
    // Clone must actually be gone before we retire the record. The
    // ordering below is retire_to_previous (renames record.json ->
    // previous.json) then clear_current. If we retired first and
    // clone-rm then failed, a retry would find current.json still
    // pointing at the rice but load_record would error on the now-
    // renamed record file — the user would be stuck with no tool
    // path forward. Fail-stop here keeps record + current.json intact
    // so retry walks the whole uninstall path again (safely: packages
    // already removed means deps::installed() returns empty; symlink
    // NotFound is idempotent-OK; rcsave gets a fresh ts+pid dir).
    //
    // `--force` skips the fail-stop: the clone is left on disk (with
    // an eprintln advising manual rm), the record is retired, and
    // the user regains tool control. Use for unremovable paths
    // (root-owned files from a rice's own hook, fs corruption).
    if clone.exists()
        && let Err(e) = remove_dir_all_forceful(&clone)
    {
        if flags.force {
            eprintln!(
                "rice-cooker: warn: --force: could not remove clone {}: {e}. Manual rm required.",
                clone.display()
            );
        } else {
            return Err(anyhow!(
                "removing clone {}: {e}\n\
                 Packages + symlink are already removed. Clear the blocker and re-run \
                 uninstall, or pass --force to orphan the clone and retire the record anyway.",
                clone.display()
            ));
        }
    }
    retire_to_previous(dirs, &name)?;
    // clear_current is the last bookkeeping step — record is already
    // retired to previous.json, packages+clone are gone. A stray
    // current.json pointing at the retired name is a reportable glitch
    // (next read_current returns Some(name), load_record errors), but
    // it's not worth re-failing the whole uninstall after success on
    // every other step. In force mode we've already accepted degraded
    // state, so warn either way and let the user move on.
    if let Err(e) = clear_current(dirs) {
        eprintln!(
            "rice-cooker: warn: could not clear current.json: {e}. \
             Remove it manually if `rice-cooker status` still reports {name} as installed."
        );
    }

    Ok(UninstallOutcome { name, rcsave_dir })
}

pub fn switch(cat: &Catalog, dirs: &Dirs, to: &str, flags: Flags) -> Result<SwitchOutcome> {
    dirs.ensure()?;
    let _lock = ApplyLock::try_acquire(&dirs.lock_file()).map_err(|e| anyhow!("lock: {e}"))?;

    // Dry-run needs its own path. If we called uninstall_locked+install_locked
    // in dry-run mode, uninstall_locked prints "would remove" but leaves
    // current.json set, then install_locked reads it as "already installed"
    // and errors out — the combined plan never prints.
    if flags.dry_run {
        let from = read_current(dirs)?.unwrap_or_default();
        let entry = cat.get(to).ok_or_else(|| anyhow!("{to}: not in catalog"))?;
        if !from.is_empty() {
            match load_record(&dirs.record_json(&from)) {
                Ok(rec) => {
                    println!("would remove symlink {}", rec.symlink_path.display());
                    if !rec.pacman_diff.added_explicit.is_empty() {
                        println!(
                            "would remove packages: {}",
                            rec.pacman_diff.added_explicit.join(" ")
                        );
                    }
                }
                Err(e) => {
                    // Surface the read failure so dry-run doesn't silently
                    // diverge from what wet `switch` would report. Wet
                    // `uninstall_locked` would hit the same error and bail.
                    eprintln!(
                        "rice-cooker: warn: cannot read current install record for {from}: {e}; \
                         dry-run omitting remove plan (wet switch would fail)"
                    );
                }
            }
        }
        let dst = expand_home(&entry.symlink_dst, &dirs.home);
        let src = dirs.clone_dir(to).join(&entry.symlink_src);
        println!("would symlink: {} -> {}", dst.display(), src.display());
        let missing_deps =
            deps::missing(&[entry.pacman_deps.clone(), entry.aur_deps.clone()].concat())?;
        if !missing_deps.is_empty() {
            println!("would install deps: {}", missing_deps.join(" "));
        } else {
            println!("deps already satisfied, zero polkit prompts");
        }
        return Ok(SwitchOutcome {
            from,
            to: to.to_string(),
            rcsave_dir: None,
        });
    }

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

fn pacman_explicit() -> Result<Vec<String>> {
    // Propagates failures: pre-snapshot failure is fatal — running an
    // install against an unreliable baseline would persist a bogus
    // pacman_diff and uninstall would either miss real packages or
    // attempt to -Rns the user's whole explicit set (depending on
    // whether pre or post failed). Better to abort the install.
    let out = Command::new("pacman")
        .args(["-Qqe"])
        .output()
        .context("running pacman -Qqe")?;
    if !out.status.success() {
        return Err(anyhow!("pacman -Qqe exited {:?}", out.status.code()));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
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

/// Remove a directory tree. Tries `std::fs::remove_dir_all` first —
/// which suffices for ordinary rice trees — and falls back to `rm
/// -rf` on PermissionDenied / NotADirectory / other kinds so we can
/// still clean makepkg's `pkg/` dir (0111 perms), stray immutable
/// files, and whatever other surprises a rice's hooks leave behind.
/// `rm -rf --` protects against clone paths that could begin with
/// `-` (our clone dirs can't, but belt-and-suspenders).
fn remove_dir_all_forceful(path: &std::path::Path) -> Result<()> {
    match std::fs::remove_dir_all(path) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {}
    }
    let status = Command::new("rm")
        .arg("-rf")
        .arg("--")
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
        .expect("RFC3339 formatting of OffsetDateTime::now_utc cannot fail")
        .replace(':', "")
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
