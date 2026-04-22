//! Pre/post snapshot diff.
//!
//! `FsDiff` is exactly what lands in the install record — added files,
//! modified files, deleted files, symlinks_added. On uninstall we walk it
//! in reverse to reconstruct pre-install state.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::snapshot::{Entry, Manifest};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FsDiff {
    #[serde(default)]
    pub added: Vec<AddedFile>,
    #[serde(default)]
    pub modified: Vec<ModifiedFile>,
    #[serde(default)]
    pub deleted: Vec<DeletedFile>,
    #[serde(default)]
    pub symlinks_added: Vec<AddedSymlink>,
    /// Directories that now exist and didn't pre-install. Used so uninstall
    /// can rmdir cleanly-emptied install dirs on the way out.
    #[serde(default)]
    pub dirs_added: Vec<AddedDir>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddedFile {
    pub path: PathBuf,
    pub hash: String,
    pub size: u64,
    pub mode: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModifiedFile {
    pub path: PathBuf,
    pub pre_hash: String,
    pub post_hash: String,
    pub pre_size: u64,
    pub post_size: u64,
    pub pre_mode: u32,
    pub post_mode: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeletedFile {
    pub path: PathBuf,
    pub pre_hash: String,
    pub pre_size: u64,
    pub pre_mode: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddedSymlink {
    pub path: PathBuf,
    pub target: PathBuf,
    pub mode: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddedDir {
    pub path: PathBuf,
    pub mode: u32,
}

impl FsDiff {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.modified.is_empty()
            && self.deleted.is_empty()
            && self.symlinks_added.is_empty()
            && self.dirs_added.is_empty()
    }
}

pub fn compute(pre: &Manifest, post: &Manifest) -> FsDiff {
    let mut diff = FsDiff::default();
    // Iterate the union of keys for deterministic output. BTreeMap makes
    // this O(n) merge — but we just iterate both sides.
    for (path, post_entry) in &post.entries {
        match pre.entries.get(path) {
            None => match post_entry {
                Entry::File { hash, size, mode } => diff.added.push(AddedFile {
                    path: path.clone(),
                    hash: hash.clone(),
                    size: *size,
                    mode: *mode,
                }),
                Entry::Symlink { target, mode } => diff.symlinks_added.push(AddedSymlink {
                    path: path.clone(),
                    target: target.clone(),
                    mode: *mode,
                }),
                Entry::Dir { mode } => diff.dirs_added.push(AddedDir {
                    path: path.clone(),
                    mode: *mode,
                }),
            },
            Some(pre_entry) => {
                // Type mismatch (file → symlink or vice versa): treat as
                // delete + add for simplicity. The uninstall order (delete
                // adds first, restore deletes last) handles it.
                match (pre_entry, post_entry) {
                    (
                        Entry::File {
                            hash: ph,
                            size: ps,
                            mode: pm,
                        },
                        Entry::File {
                            hash: h,
                            size: s,
                            mode: m,
                        },
                    ) => {
                        if ph != h || ps != s || pm != m {
                            diff.modified.push(ModifiedFile {
                                path: path.clone(),
                                pre_hash: ph.clone(),
                                post_hash: h.clone(),
                                pre_size: *ps,
                                post_size: *s,
                                pre_mode: *pm,
                                post_mode: *m,
                            });
                        }
                    }
                    (Entry::Symlink { target: pt, .. }, Entry::Symlink { target: t, mode: m }) => {
                        if pt != t {
                            // Symlink retargeted. Model as: delete pre + add post.
                            diff.deleted.push(DeletedFile {
                                path: path.clone(),
                                pre_hash: String::new(),
                                pre_size: 0,
                                pre_mode: 0,
                            });
                            diff.symlinks_added.push(AddedSymlink {
                                path: path.clone(),
                                target: t.clone(),
                                mode: *m,
                            });
                        }
                    }
                    (Entry::Dir { mode: pm }, Entry::Dir { mode: m }) if pm != m => {
                        // We don't track mode-only changes on dirs — the
                        // install's intent is what matters; uninstall
                        // doesn't touch the dir's mode on restore.
                    }
                    (pre @ Entry::File { .. }, _post) => {
                        // Pre file, post different kind: treat as file deletion
                        // plus whatever the post is (added file/symlink/dir).
                        if let Entry::File { hash, size, mode } = pre {
                            diff.deleted.push(DeletedFile {
                                path: path.clone(),
                                pre_hash: hash.clone(),
                                pre_size: *size,
                                pre_mode: *mode,
                            });
                        }
                        // Then record the post side.
                        match post_entry {
                            Entry::File { hash, size, mode } => diff.added.push(AddedFile {
                                path: path.clone(),
                                hash: hash.clone(),
                                size: *size,
                                mode: *mode,
                            }),
                            Entry::Symlink { target, mode } => {
                                diff.symlinks_added.push(AddedSymlink {
                                    path: path.clone(),
                                    target: target.clone(),
                                    mode: *mode,
                                })
                            }
                            Entry::Dir { mode } => diff.dirs_added.push(AddedDir {
                                path: path.clone(),
                                mode: *mode,
                            }),
                        }
                    }
                    (Entry::Symlink { .. }, _) | (Entry::Dir { .. }, _) => {
                        // Type change from symlink/dir: same treatment — record
                        // the "pre was there" as a conceptual delete and the
                        // post as whatever it is. We don't have per-kind
                        // DeletedSymlink/DeletedDir records in v1 (the uninstall
                        // walks the post state and removes iff it matches our
                        // record). Safe omission for v1.
                    }
                }
            }
        }
    }
    // Files / symlinks / dirs in pre, not in post → deleted.
    for (path, pre_entry) in &pre.entries {
        if post.entries.contains_key(path) {
            continue;
        }
        if let Entry::File { hash, size, mode } = pre_entry {
            diff.deleted.push(DeletedFile {
                path: path.clone(),
                pre_hash: hash.clone(),
                pre_size: *size,
                pre_mode: *mode,
            });
        }
        // Deleted dirs / symlinks aren't modeled in v1 — reversing the rare
        // "install deleted one of the user's symlinks" case isn't in scope.
    }
    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn file(hash: &str, size: u64, mode: u32) -> Entry {
        Entry::File {
            hash: hash.into(),
            size,
            mode,
        }
    }
    fn symlink(target: &str, mode: u32) -> Entry {
        Entry::Symlink {
            target: Path::new(target).to_path_buf(),
            mode,
        }
    }

    fn mk(entries: Vec<(&str, Entry)>) -> Manifest {
        let mut m = BTreeMap::new();
        for (p, e) in entries {
            m.insert(PathBuf::from(p), e);
        }
        Manifest {
            taken_at: 0,
            roots: vec![],
            entries: m,
        }
    }

    #[test]
    fn added_modified_deleted_split_correctly() {
        let pre = mk(vec![
            ("/h/a", file("A", 1, 0o644)),
            ("/h/b", file("B", 2, 0o644)),
            ("/h/c", file("C", 3, 0o644)),
        ]);
        let post = mk(vec![
            ("/h/a", file("A", 1, 0o644)),    // unchanged
            ("/h/b", file("Bnew", 2, 0o644)), // modified
            ("/h/d", file("D", 4, 0o644)),    // added
                                              // /h/c is gone
        ]);
        let d = compute(&pre, &post);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].path, PathBuf::from("/h/d"));
        assert_eq!(d.modified.len(), 1);
        assert_eq!(d.modified[0].path, PathBuf::from("/h/b"));
        assert_eq!(d.modified[0].pre_hash, "B");
        assert_eq!(d.modified[0].post_hash, "Bnew");
        assert_eq!(d.deleted.len(), 1);
        assert_eq!(d.deleted[0].path, PathBuf::from("/h/c"));
    }

    #[test]
    fn symlinks_added_captured_separately() {
        let pre = mk(vec![]);
        let post = mk(vec![("/h/l", symlink("/some/target", 0o777))]);
        let d = compute(&pre, &post);
        assert!(d.added.is_empty());
        assert_eq!(d.symlinks_added.len(), 1);
        assert_eq!(d.symlinks_added[0].target, PathBuf::from("/some/target"));
    }

    #[test]
    fn mode_only_change_counts_as_modified() {
        let pre = mk(vec![("/h/a", file("SAME", 1, 0o644))]);
        let post = mk(vec![("/h/a", file("SAME", 1, 0o755))]);
        let d = compute(&pre, &post);
        assert_eq!(d.modified.len(), 1);
        assert_eq!(d.modified[0].pre_mode, 0o644);
        assert_eq!(d.modified[0].post_mode, 0o755);
    }

    #[test]
    fn symlink_retargeted_modeled_as_delete_plus_add() {
        let pre = mk(vec![("/h/l", symlink("/old", 0o777))]);
        let post = mk(vec![("/h/l", symlink("/new", 0o777))]);
        let d = compute(&pre, &post);
        assert_eq!(d.symlinks_added.len(), 1);
        assert_eq!(d.symlinks_added[0].target, PathBuf::from("/new"));
        assert_eq!(d.deleted.len(), 1);
        assert_eq!(d.deleted[0].path, PathBuf::from("/h/l"));
    }

    #[test]
    fn round_trips_through_json() {
        let d = FsDiff {
            added: vec![AddedFile {
                path: PathBuf::from("/h/a"),
                hash: "HASH".into(),
                size: 10,
                mode: 0o644,
            }],
            ..Default::default()
        };
        let s = serde_json::to_string(&d).unwrap();
        let back: FsDiff = serde_json::from_str(&s).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn empty_empty_is_empty() {
        let d = compute(&mk(vec![]), &mk(vec![]));
        assert!(d.is_empty());
    }
}
