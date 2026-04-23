//! install / uninstall / switch / list / status — symlink-only pipeline.
//!
//! Install = clone rice at pinned commit → paru installs deps via pkexec
//! → ln -sfnT into the clone → write install record. Uninstall = remove
//! deps → rm symlink → rm clone → delete record.

use std::fs;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

use crate::catalog::{Catalog, is_placeholder_commit};
use crate::deps;
use crate::git;
use crate::lock::ApplyLock;
use crate::paths::{Paths, expand_home};

use super::record::{
    InstallRecord, PacmanDiff, SCHEMA_VERSION, clear_current, load_record, read_current,
    save_record, write_current,
};
use super::symlink as symlink_shape;

#[derive(Debug, Clone, Copy, Default)]
pub struct Flags {
    pub force: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallOutcome {
    pub name: String,
    pub pacman_diff: PacmanDiff,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UninstallOutcome {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SwitchOutcome {
    pub from: String,
    pub to: String,
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

pub fn install(cat: &Catalog, paths: &Paths, name: &str, _flags: Flags) -> Result<InstallOutcome> {
    paths.ensure_rices()?;
    paths.ensure_installs()?;
    let _lock = ApplyLock::try_acquire(&paths.lock()).map_err(|e| anyhow!("lock: {e}"))?;
    install_locked(cat, paths, name)
}

fn install_locked(cat: &Catalog, paths: &Paths, name: &str) -> Result<InstallOutcome> {
    let entry = cat
        .get(name)
        .ok_or_else(|| anyhow!("{name}: not in catalog"))?;

    if is_placeholder_commit(&entry.commit) {
        return Err(anyhow!(
            "{name}: catalog commit is a placeholder ({}). Pin a real SHA in catalog.toml before installing.",
            entry.commit
        ));
    }

    if let Some(cur) = read_current(paths)? {
        return Err(anyhow!(
            "{cur} is already installed — run uninstall or switch first"
        ));
    }

    // Clone / re-clone.
    let clone = paths.clone_dir(name)?;
    if clone.exists() {
        remove_dir_all_forceful(&clone)
            .with_context(|| format!("removing stale clone {}", clone.display()))?;
    }
    eprintln!("rice-cooker: cloning {} @ {}", entry.repo, entry.commit);
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
        eprintln!("rice-cooker: install deps: {}", missing_deps.join(" "));
        deps::install_packages(&missing_deps)?;
    } else if !all_deps.is_empty() {
        eprintln!("rice-cooker: deps already satisfied");
    }

    let post_explicit = pacman_explicit().context("pacman -Qqe post-snapshot")?;
    let added_explicit = diff_explicit(&pre_explicit, &post_explicit);

    // Persist ownership BEFORE create_symlink so a symlink failure still
    // leaves a record that uninstall can use to remove the just-installed
    // packages. Symlink paths are deterministic from the entry + home,
    // so we can compute them without actually making the symlink yet.
    let symlink_path = expand_home(&entry.symlink_dst, &paths.home);
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
    save_record(&paths.record_json(name)?, &record)?;
    write_current(paths, name)?;

    // Now create the symlink. If it fails, the record is already on
    // disk so uninstall can roll back the packages — but the user
    // hitting this mid-install has no idea they need to uninstall;
    // `install <name>` would just keep erroring with "already
    // installed". Wrap the error with a remediation hint.
    if let Err(e) = symlink_shape::create_symlink(&clone, entry, &paths.home) {
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
    })
}

pub fn uninstall(paths: &Paths, flags: Flags) -> Result<UninstallOutcome> {
    paths.ensure_rices()?;
    paths.ensure_installs()?;
    let _lock = ApplyLock::try_acquire(&paths.lock()).map_err(|e| anyhow!("lock: {e}"))?;
    uninstall_locked(paths, flags)
}

fn uninstall_locked(paths: &Paths, flags: Flags) -> Result<UninstallOutcome> {
    let name = read_current(paths)?.ok_or_else(|| anyhow!("no rice installed"))?;
    let record = load_record(&paths.record_json(&name)?)?;

    // Remove packages. Pre-filter already-removed so pacman doesn't
    // abort with "target not found" on retry. A pacman-query failure
    // here propagates — silently skipping removal would strand the
    // rice's packages on disk with no tool-visible owner.
    if !record.pacman_diff.added_explicit.is_empty() {
        let still_installed = deps::installed(&record.pacman_diff.added_explicit)?;
        if !still_installed.is_empty() {
            eprintln!(
                "rice-cooker: remove packages: {}",
                still_installed.join(" ")
            );
            deps::remove_packages(&still_installed)?;
        }
    }

    let clone = paths.clone_dir(&name)?;

    // Remove symlink — but only if it's still OUR symlink. User could
    // have replaced it with a regular file/dir since install; we don't
    // want to clobber that. NotFound is idempotent-OK; type mismatch or
    // target mismatch is skipped with a warning that includes both
    // paths for diagnostics; real IO errors bubble up.
    //
    // If we skip (retargeted/replaced), the record is still removed +
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
    // Clone must actually be gone before we delete the record. If we
    // deleted the record first and clone-rm then failed, a retry would
    // find current.json still pointing at the rice but load_record
    // would error on the missing record file — the user would be stuck
    // with no tool path forward. Fail-stop here keeps record +
    // current.json intact so retry walks the whole uninstall path
    // again (safely: packages already removed means deps::installed()
    // returns empty; symlink NotFound is idempotent-OK).
    //
    // `--force` skips the fail-stop: the clone is left on disk (with
    // an eprintln advising manual rm), the record is deleted, and the
    // user regains tool control. Use for unremovable paths (root-owned
    // files from a rice's own hook, fs corruption).
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
                 uninstall, or pass --force to orphan the clone and delete the record anyway.",
                clone.display()
            ));
        }
    }
    // Delete the record. NotFound is idempotent-OK.
    let record_path = paths.record_json(&name)?;
    match fs::remove_file(&record_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(anyhow!("removing record {}: {e}", record_path.display()));
        }
    }
    // clear_current is the last bookkeeping step — record is already
    // gone, packages+clone are gone. A stray current.json pointing at
    // the deleted name is a reportable glitch (next read_current
    // returns Some(name), load_record errors), but it's not worth
    // re-failing the whole uninstall after success on every other
    // step. In force mode we've already accepted degraded state, so
    // warn either way and let the user move on.
    if let Err(e) = clear_current(paths) {
        eprintln!(
            "rice-cooker: warn: could not clear current.json: {e}. \
             Remove it manually if `rice-cooker status` still reports {name} as installed."
        );
    }

    Ok(UninstallOutcome { name })
}

pub fn switch(cat: &Catalog, paths: &Paths, to: &str, flags: Flags) -> Result<SwitchOutcome> {
    paths.ensure_rices()?;
    paths.ensure_installs()?;
    let _lock = ApplyLock::try_acquire(&paths.lock()).map_err(|e| anyhow!("lock: {e}"))?;
    let uo = uninstall_locked(paths, flags)?;
    let io = install_locked(cat, paths, to)?;
    Ok(SwitchOutcome {
        from: uo.name,
        to: io.name,
    })
}

pub fn list(cat: &Catalog, paths: &Paths) -> Result<Vec<ListRow>> {
    let current = read_current(paths)?;
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

pub fn status(paths: &Paths) -> Result<StatusRow> {
    let name = match read_current(paths)? {
        Some(n) => n,
        None => return Ok(StatusRow { installed: None }),
    };
    let record = load_record(&paths.record_json(&name)?)?;
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
    let fs_err = match std::fs::remove_dir_all(path) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => e,
    };
    // Fallback to `rm -rf`. Preserve the original std::fs error in the
    // final message so a failed fallback shows both diagnostics — the
    // Rust-side kind (PermissionDenied, DirectoryNotEmpty, etc.) tells
    // us what the stdlib hit, the rm exit code tells us what `rm`
    // saw on its second pass.
    let status = Command::new("rm")
        .arg("-rf")
        .arg("--")
        .arg(path)
        .status()
        .map_err(|rm_err| {
            anyhow!(
                "spawning rm -rf {}: {rm_err} (after std::fs::remove_dir_all failed: {fs_err})",
                path.display()
            )
        })?;
    if !status.success() {
        return Err(anyhow!(
            "rm -rf {} exited {:?} (after std::fs::remove_dir_all failed: {fs_err})",
            path.display(),
            status.code()
        ));
    }
    Ok(())
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
