//! Pre/post filesystem snapshot: walk the watched roots, hash every
//! regular file with BLAKE3, record symlink targets, note directories.
//!
//! A snapshot is a `Manifest` — a flat map from absolute path to `Entry`.
//! Two manifests are diffed to produce an `FsDiff` (see `diff.rs`).

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};

/// Default watched roots relative to `$HOME`. Chosen to cover everywhere a
/// Quickshell rice is expected to deploy.
///
/// We deliberately do NOT watch all of `.local/share`: on a seasoned
/// system that tree holds gigabytes of per-app state (Steam, Flatpak,
/// Electron-app DBs, browser profiles) that no Quickshell rice touches.
/// Wholesale pre-content backup across all of `.local/share` would stall
/// install for minutes and burn disk equal to the copied tree. We instead
/// list the `.local/share` subdirs rices typically deploy into
/// (applications, icons, fonts, themes, plus `quickshell` itself) and
/// invite rices with different needs to declare `extra_watched_roots` in
/// their catalog entry.
pub const DEFAULT_WATCHED_ROOTS: &[&str] = &[
    ".config",
    ".local/bin",
    ".local/lib",
    ".local/share/applications",
    ".local/share/fonts",
    ".local/share/icons",
    ".local/share/quickshell",
    ".local/share/themes",
];

/// Path suffixes (relative to HOME) that never enter a snapshot regardless
/// of root. Runtime caches + VCS metadata + systemd timer state + our own
/// tool's cache (so we don't recurse).
pub const DEFAULT_EXCLUDES: &[&str] = &[
    ".cache",
    ".local/share/Trash",
    ".local/share/systemd/timers",
    ".local/state",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// ISO-8601-ish seconds (we just store a u64 for simplicity; record.rs
    /// formats humanly).
    pub taken_at: u64,
    pub roots: Vec<PathBuf>,
    pub entries: BTreeMap<PathBuf, Entry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    File {
        hash: String, // blake3 hex
        size: u64,
        /// Permission bits only (0o7777 mask).
        mode: u32,
    },
    Symlink {
        /// The symlink's own target, not followed.
        target: PathBuf,
        mode: u32,
    },
    Dir {
        mode: u32,
    },
}

impl Entry {
    pub fn is_file(&self) -> bool {
        matches!(self, Entry::File { .. })
    }
    pub fn is_symlink(&self) -> bool {
        matches!(self, Entry::Symlink { .. })
    }
    pub fn is_dir(&self) -> bool {
        matches!(self, Entry::Dir { .. })
    }
}

/// Options controlling a snapshot walk. Constructed per install so catalog
/// `extra_watched_roots` can be folded in.
#[derive(Debug, Clone)]
pub struct WalkOpts {
    /// Absolute paths of root directories to walk.
    pub roots: Vec<PathBuf>,
    /// Absolute paths (prefix match) to skip entirely.
    pub excludes: Vec<PathBuf>,
    /// Individual file paths to snapshot regardless of root coverage. Lets
    /// catalog `partial_ownership` and `runtime_regenerated` declare HOME-
    /// level files (like `~/.zshrc`) without forcing the default walk to
    /// cover all of $HOME. Each listed path is stat'd once; missing paths
    /// are silently skipped (the rice may not have touched it yet).
    pub extra_files: Vec<PathBuf>,
    /// Skip files larger than this many bytes entirely (no hash, no record).
    /// 128 MiB default — no rice ships a single file that big.
    pub max_file_bytes: u64,
}

impl WalkOpts {
    /// Build walk opts for a given HOME, layering in the catalog extras.
    /// `extras` are catalog strings like `~/Pictures/wallpapers`.
    pub fn for_home(home: &Path, extras: &[String]) -> Result<Self> {
        Self::for_home_with_extras(home, extras, &[], &[])
    }

    /// Like `for_home`, plus explicit single-file paths that must be
    /// tracked regardless of which roots are walked. Used by the install
    /// pipeline to cover catalog `partial_ownership` + `runtime_regenerated`.
    pub fn for_home_with_extras(
        home: &Path,
        extra_roots: &[String],
        partial_ownership: &[String],
        runtime_regenerated: &[String],
    ) -> Result<Self> {
        use super::env::expand_home;
        let mut roots: Vec<PathBuf> = DEFAULT_WATCHED_ROOTS
            .iter()
            .map(|r| home.join(r))
            .collect();
        for extra in extra_roots {
            let p = expand_home(extra, home);
            if !p.starts_with(home) {
                return Err(anyhow!(
                    "extra_watched_root {extra:?} resolves outside $HOME"
                ));
            }
            roots.push(p);
        }
        let excludes: Vec<PathBuf> = DEFAULT_EXCLUDES.iter().map(|e| home.join(e)).collect();
        let mut extra_files: Vec<PathBuf> = Vec::new();
        for s in partial_ownership.iter().chain(runtime_regenerated.iter()) {
            let p = expand_home(s, home);
            if !p.starts_with(home) {
                return Err(anyhow!(
                    "catalog path {s:?} resolves outside $HOME"
                ));
            }
            extra_files.push(p);
        }
        Ok(WalkOpts {
            roots,
            excludes,
            extra_files,
            max_file_bytes: 128 * 1024 * 1024,
        })
    }
}

pub fn take_snapshot(opts: &WalkOpts) -> Result<Manifest> {
    let taken_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut entries = BTreeMap::new();
    for root in &opts.roots {
        walk_root(root, opts, &mut entries)?;
    }
    // Extra single-file paths (catalog's partial_ownership /
    // runtime_regenerated). Only snapshot if not already covered by a
    // root's walk — BTreeMap::insert would overwrite, but the hash would
    // be identical so it's semantically fine.
    for p in &opts.extra_files {
        if entries.contains_key(p) {
            continue;
        }
        let meta = match fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(_) => continue, // path doesn't exist yet — pre-install OK
        };
        let mode = meta.permissions().mode() & 0o7777;
        let ft = meta.file_type();
        let record = if ft.is_symlink() {
            let target = fs::read_link(p)
                .with_context(|| format!("readlink {}", p.display()))?;
            Entry::Symlink { target, mode }
        } else if ft.is_file() {
            if meta.len() > opts.max_file_bytes {
                continue;
            }
            let hash = hash_file(p)?;
            Entry::File {
                hash,
                size: meta.len(),
                mode,
            }
        } else {
            continue;
        };
        entries.insert(p.clone(), record);
    }
    Ok(Manifest {
        taken_at,
        roots: opts.roots.clone(),
        entries,
    })
}

fn walk_root(
    root: &Path,
    opts: &WalkOpts,
    entries: &mut BTreeMap<PathBuf, Entry>,
) -> Result<()> {
    // Missing root is fine — treat as empty. A rice might not deploy to
    // ~/.local/bin, for instance.
    if !root.exists() {
        return Ok(());
    }
    // jwalk follows symlinks into dirs by default — we DO NOT want that for
    // snapshots. Configure it to NOT follow links and record symlinks in
    // place.
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        // .skip_hidden is wrong for our case — we DO want dotfiles.
        .skip_hidden(false);
    for result in walker {
        let entry = match result {
            Ok(e) => e,
            // Permission denied or other transient error on one file —
            // log and keep walking. A missing file that existed at walk
            // start would fail with NotFound; we tolerate all these the
            // same way.
            Err(err) => {
                eprintln!("snapshot: skipping {err}");
                continue;
            }
        };
        let path = entry.path();
        if is_excluded(&path, opts, root) {
            // jwalk doesn't have a built-in exclude; we just skip the
            // entry and let the walk continue. jwalk recurses into dirs
            // it finds; to stop recursion we'd need a DirEntryFilter
            // callback. For now we walk fully and filter per-entry.
            continue;
        }
        // symlink_metadata so we don't follow the link.
        let meta = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(e).with_context(|| format!("stat {}", path.display()));
            }
        };
        let mode = meta.permissions().mode() & 0o7777;
        let ft = meta.file_type();
        let record = if ft.is_symlink() {
            let target = fs::read_link(&path)
                .with_context(|| format!("readlink {}", path.display()))?;
            Entry::Symlink { target, mode }
        } else if ft.is_dir() {
            Entry::Dir { mode }
        } else if ft.is_file() {
            if meta.len() > opts.max_file_bytes {
                // Too big to hash — skip with a warning. The install won't
                // cover it, so uninstall won't touch it either.
                eprintln!(
                    "snapshot: skipping {} ({} bytes > limit)",
                    path.display(),
                    meta.len()
                );
                continue;
            }
            let hash = hash_file(&path)?;
            Entry::File {
                hash,
                size: meta.len(),
                mode,
            }
        } else {
            // Sockets, fifos, devices — we don't track.
            continue;
        };
        entries.insert(path, record);
    }
    Ok(())
}

fn is_excluded(path: &Path, opts: &WalkOpts, _root: &Path) -> bool {
    // Exact or prefix match on any exclude.
    for ex in &opts.excludes {
        if path == ex || path.starts_with(ex) {
            return true;
        }
    }
    // Any `.git` directory (spec excludes nested VCS). Check any component.
    for c in path.components() {
        if c.as_os_str() == ".git" {
            return true;
        }
    }
    false
}

pub fn hash_file(path: &Path) -> Result<String> {
    let f = fs::File::open(path)
        .with_context(|| format!("opening {} for hash", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut reader = BufReader::new(f);
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .with_context(|| format!("reading {} for hash", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute an all-of-path-bytes sha256-style safe filename for a content
/// backup. Hash is over the absolute path string, used purely as a
/// filename-safe key (per the spec). Independent of file content.
pub fn path_key(path: &Path) -> String {
    let bytes = path.as_os_str().as_encoded_bytes();
    let hash = blake3::hash(bytes);
    hash.to_hex().to_string()
}

/// Persist a manifest to disk as pretty JSON, atomic temp+rename.
pub fn save_manifest(path: &Path, m: &Manifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(m).context("serializing manifest")?;
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, body.as_bytes())
        .with_context(|| format!("writing {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

pub fn load_manifest(path: &Path) -> Result<Manifest> {
    let body = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&body).context("parsing manifest JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink as unix_symlink;
    use tempfile::tempdir;

    fn touch(path: &Path, body: &[u8], mode: u32) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, body).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(mode);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn snapshot_records_files_dirs_symlinks_skips_excludes() {
        let tmp = tempdir().unwrap();
        let home = tmp.path();
        // Fake $HOME with a .config/hypr/hyprland.conf, a .cache file (excluded),
        // a .local/bin/script, a symlink into cache, a nested .git.
        touch(&home.join(".config/hypr/hyprland.conf"), b"hyprland", 0o644);
        touch(&home.join(".local/bin/script"), b"#!/bin/sh", 0o755);
        touch(&home.join(".cache/junk"), b"transient", 0o644);
        touch(&home.join(".config/something/.git/HEAD"), b"ref", 0o644);
        fs::create_dir_all(home.join(".config/quickshell")).unwrap();
        unix_symlink(
            home.join(".cache/real"),
            home.join(".config/quickshell/noctalia"),
        )
        .unwrap();

        let opts = WalkOpts::for_home(home, &[]).unwrap();
        let manifest = take_snapshot(&opts).unwrap();

        // hyprland.conf is there as a file.
        let hypr = home.join(".config/hypr/hyprland.conf");
        let e = manifest.entries.get(&hypr).unwrap();
        assert!(e.is_file(), "hypr not a file: {e:?}");
        // bin/script too.
        assert!(manifest
            .entries
            .get(&home.join(".local/bin/script"))
            .unwrap()
            .is_file());
        // symlink is present and not followed.
        let sym = home.join(".config/quickshell/noctalia");
        let e = manifest.entries.get(&sym).unwrap();
        assert!(e.is_symlink(), "expected symlink, got {e:?}");
        // .cache/junk excluded.
        assert!(!manifest.entries.contains_key(&home.join(".cache/junk")));
        // .git/HEAD excluded via component filter.
        assert!(!manifest
            .entries
            .contains_key(&home.join(".config/something/.git/HEAD")));
        // The .git dir itself is also filtered.
        assert!(!manifest
            .entries
            .contains_key(&home.join(".config/something/.git")));
    }

    #[test]
    fn same_content_same_hash() {
        let tmp = tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::write(&a, b"payload").unwrap();
        fs::write(&b, b"payload").unwrap();
        assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let tmp = tempdir().unwrap();
        touch(&tmp.path().join(".config/x.conf"), b"x", 0o644);
        let opts = WalkOpts::for_home(tmp.path(), &[]).unwrap();
        let m = take_snapshot(&opts).unwrap();
        let path = tmp.path().join("manifest.json");
        save_manifest(&path, &m).unwrap();
        let back = load_manifest(&path).unwrap();
        assert_eq!(back.entries.len(), m.entries.len());
        assert_eq!(back.taken_at, m.taken_at);
    }

    #[test]
    fn extra_watched_root_outside_home_rejected() {
        let r = WalkOpts::for_home(
            Path::new("/home/x"),
            &["/etc/hypr".into()],
        );
        assert!(r.is_err());
    }

    #[test]
    fn extra_watched_root_inside_home_accepted() {
        let r = WalkOpts::for_home(
            Path::new("/home/x"),
            &["~/Pictures/wallpapers".into()],
        )
        .unwrap();
        assert!(r.roots.contains(&PathBuf::from("/home/x/Pictures/wallpapers")));
    }
}
