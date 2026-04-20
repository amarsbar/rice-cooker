# rice-cooker-backend: install/uninstall design (v1 spike)

## Scope

Persist a curated Quickshell rice so `qs -c <id>` can load it without re-running
`apply`. Reverse cleanly on `uninstall`. Works on the 14 rices that already pass
`apply`. Local-only — no pacman invocations, no systemd, no hooks in v1.

## Scrutiny of the auto-generated guide

The guide proposed a large pacman/systemd/SQLite stack. For v1, most of it is
unnecessary or premature:

- **Meta-package generation via alpm-package** — dropped. The 14 target rices
  already run under `apply`, so all deps are on the system. A rice's catalog
  entry lists suggested pacman packages for the user's benefit; we don't
  invoke pacman.
- **SQLite WAL journal via rusqlite** — dropped. Plain JSON at
  `$XDG_STATE_HOME/rice-cooker/installs.json`, atomic temp+rename writes.
  Single-user, single-process. Rusqlite pulls 100k+ LOC of deps that buy
  nothing here.
- **Per-file copy/symlink classification matrix** — deferred. The v1 default
  rule (symlink the rice's shell directory) covers all 14 rices; the richer
  matrix is needed for desktop-suite rices (GTK/Qt/btop configs) that v1
  doesn't deploy.
- **systemd user units, reload hooks** — dropped. None of the 14 rices need
  them in v1 (they run as standalone `quickshell` shells).
- **Content-addressed multi-version store** — dropped. Single-version in
  place; journal records the prior state so rollback works.
- **Switch command** — dropped. User does `uninstall` + `install`.

Retained from the guide:
- Per-operation state machine: PLANNED → STARTED → COMMITTED.
- Atomic temp+rename for file ops; fsync the parent dir.
- Central backup store (not inline `.bak` siblings).
- Hash-based user-modification detection (record sha256 at install time so
  uninstall can refuse to delete modified files).

## Architecture

### Catalog

Ships in-tree at `backend/catalog/<id>.toml`, one file per rice. Minimum:

```toml
[rice]
id = "caelestia"
name = "Caelestia"
upstream = "https://github.com/caelestia-dots/shell"
# Defaults to "shell.qml" if absent.
entry = "shell.qml"
```

Optional extensions (not needed by v1 defaults):

```toml
[dependencies]
pacman = ["quickshell", "qt6-5compat"]   # advisory only; printed to user

[[files]]
src  = "quickshell"                      # repo-relative
dest = "$XDG_CONFIG_HOME/quickshell/caelestia"
mode = "symlink"                         # "symlink" or "copy"
```

If no `[[files]]` is present, the default rule applies: symlink the parent
directory of `entry` to `$XDG_CONFIG_HOME/quickshell/<id>/`.

### Journal

`$XDG_STATE_HOME/rice-cooker/installs.json` (falls back to
`~/.local/state/rice-cooker/installs.json`). One `InstallRecord` per
install attempt, keyed by `rice_id`. Structure:

```json
{
  "schema_version": 1,
  "records": [
    {
      "rice_id": "caelestia",
      "state": "installed",
      "installed_at": 1745200000,
      "source_sha": "a1b2c3d...",
      "operations": [
        {
          "seq": 0,
          "kind": "backup_move",
          "state": "committed",
          "abs_path": "/home/x/.config/quickshell/caelestia",
          "backup_path": "/home/x/.local/share/rice-cooker/backups/caelestia/1745200000/quickshell_caelestia",
          "was_symlink": false,
          "mode": 493,
          "sha256": "deadbeef..."
        },
        {
          "seq": 1,
          "kind": "symlink_create",
          "state": "committed",
          "abs_path": "/home/x/.config/quickshell/caelestia",
          "symlink_target": "/home/x/.cache/rice-cooker/rices/caelestia/quickshell"
        }
      ]
    }
  ]
}
```

When `state` transitions to `uninstalled`, operations are rolled back in
reverse seq order and each op's state becomes `rolled_back`.

### File deployment primitives

Three atomic primitives, all crash-safe:

1. `symlink_replace(target, dest) -> BackupRecord?` — if dest exists, move
   it to backup dir. Then create symlink atomically via tmp+rename.
2. `copy_file_replace(src, dest) -> BackupRecord?` — same, but copies.
3. `restore(abs_path, backup_record)` — inverse of (1) and (2). Verifies the
   current state matches what we installed (by sha256 for copies, by
   readlink for symlinks) before overwriting.

### CLI

```
rice-cooker-backend install --id <rice-id>
rice-cooker-backend uninstall --id <rice-id>
rice-cooker-backend list-installed
rice-cooker-backend status  # existing; extended to show installed count
```

Install reuses `git::clone_or_update` into the same cache path as `apply`
(`$CACHE/rices/<id>`). Apply and install can coexist: the cache clone is
shared, and the symlink from `~/.config/quickshell/<id>/` points into it.

### Failure handling

Install pipeline uses the same `try_stage!` + hello/fail/success NDJSON
contract as apply. New `Step` variants: `ReadCatalog`, `Deploy`. New stages:
`catalog`, `deploy`, `journal`.

Partial-install rollback: if deploy fails mid-pipeline, reverse-walk the
committed operations of the *current* transaction before emitting the fail
event. The journal is left with a `partial` record whose remaining ops are
`rolled_back`, so future uninstall is a no-op on that record.

## Test plan

- **Catalog**: parse minimal + full, reject missing fields, default entry.
- **Journal**: roundtrip, atomic write under crash injection, concurrent-read
  safety (reads tolerate missing/partial file via temp+rename).
- **Deploy**: symlink to new path, symlink over existing file (backup +
  restore), symlink over existing symlink, copy variant.
- **Install happy path**: catalog + clone + deploy + journal updated.
- **Install over existing `~/.config/quickshell/<id>`**: backs up then
  symlinks.
- **Uninstall**: reverse walk, backup restored, journal marked uninstalled.
- **Uninstall with user-modified file**: refuses to delete (leaves in place,
  emits warning).
- **14-rice integration**: for each, with fake git populating the rice
  source from a fixture directory: `install` → assert symlink points at
  cache, assert journal record exists, `uninstall` → assert symlink gone,
  assert journal marked uninstalled.

## Non-goals for v1

- pacman interaction (user manages deps)
- systemd user units
- reload hooks
- `switch` command (do uninstall + install)
- Multi-version store
- Templated configs
- Scripts execution from catalog
- D-Bus / daemon mode
