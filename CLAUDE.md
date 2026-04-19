# CLAUDE.md

Guidance for Claude Code (and other AI assistants) working in this repo.

## Project shape

- Electron + Vite + React frontend in `/electron`, `/src`, `/index.html`. Linux-only (Wayland/Hyprland targets).
- Rust CLI backend in `/backend` — a thin orchestration layer around `git`, `pkill`, `pgrep`, `setsid`, and `quickshell`. Streams NDJSON on stdout.
- One `main` branch. Work lives on feature branches; PRs target `main`.

## Testing philosophy

This backend is ~1200 LOC of glue over external processes. That shape doesn't warrant heavy test weight.

- **Prefer fewer, higher-value tests.** A 1:1 tests-to-code ratio for this kind of backend is a smell, not a target. Reference CLI tools in Rust (cargo-binstall, mise, rustup) sit around 25–40% test share.
- **Don't write tests for thin wrappers.** If the function is `let out = Command::new("pkill").args(...).status()?; map_exit_code(out)`, testing it with a fake binary is testing the fake, not the code.
- **One integration test per user-visible failure mode, not per branch.** Five tests for five `if let Err(e) = X { emit_fail(stage, ...); return Ok(false) }` sites have the same shape — they verify that the macro/helper emits the right event. One parameterized test covers that.
- **Keep tests for genuine logic complexity.** QML import classification (`detect.rs`), path validation (`validate_rice_name`), atomic state-file swap (`cache.rs::swap_active_previous`), NDJSON schema round-trip — these earn coverage.
- **Negative-assertion tests are load-bearing.** A test that asserts "after precheck fails, `pkill` was NEVER invoked" is irreplaceable by any positive test.

Rule of thumb: if you'd catch the regression on first manual run (or in clippy), the test isn't pulling its weight.

## Code style

- **No Co-Authored-By in commit messages, PR descriptions, or issue comments.** Project convention.
- **Comments explain WHY, not WHAT.** Don't restate the function name or paraphrase the code. Do explain: non-obvious invariants, historical context for a design choice, and load-bearing ordering (e.g. "this check must come before kill() because ...").
- **Errors bubble with `anyhow::Context`, not bare `?`.** For user-facing failure paths on the apply/revert/exit codepath, use the `try_stage!` macro (emits a fail event and returns Ok(false) — never exit-code-2 without a fail event).
- **Defense-in-depth: prefer one correct layer over two redundant ones.** E.g. don't both `if starts_with("ext::")` AND `-c protocol.ext.allow=never` — the config flag is the stronger, complete defense.

## Module granularity

Don't split tiny files. A module earns its own file when it has a distinct conceptual boundary AND more than ~50 lines of real content. 17-line "entry.rs" that's only ever called from one other file belongs in that other file.

Current backend layout (9 files in `src/`):
- `apply.rs` — orchestration, name validation, shell-qml resolution
- `cache.rs` — XDG cache root + single-line state files + atomic write
- `detect.rs` — QML import classification
- `events.rs` — NDJSON Event enum + writer
- `git.rs` — shell out to system git (with hardening env)
- `lock.rs` — flock on apply.lock
- `process.rs` — pkill/pgrep/setsid wrappers + `/proc` introspection
- `lib.rs`, `main.rs` — re-exports and CLI dispatch

If a new concern fits one of those, extend it. Don't add a new file.

## Rust specifics

- `cargo fmt` + `cargo clippy --all-targets --all-features -- -D warnings` must be clean before committing.
- `Cargo.lock` is checked in (binary crate). `backend/target/` is gitignored.
- Keep dependencies minimal. Prefer 25 lines of manual `Display`/`Error` over adding `thiserror`.

## Linux-only

No `#![cfg(unix)]` gating needed — the crate uses `libc`, `setsid`, `pkill`, `pgrep`, flock, and `/proc` throughout. If a Windows/macOS port ever happens, it starts with a platform shim, not a conditional per file.
