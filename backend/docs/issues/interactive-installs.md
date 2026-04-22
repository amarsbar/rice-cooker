# Support interactive rice installers

**Labels:** `enhancement`, `post-v1`, `catalog`

## Context

v1 supports only rices whose `install_cmd` runs non-interactively — stdin is
closed (`/dev/null`) and the script must complete without any prompts. This
covers Caelestia, end-4's current Quickshell branch, DankMaterialShell,
Noctalia, and most Quickshell-native rices whose install is essentially
"install deps, drop QML files in place."

It does not cover rices whose installers ask questions:

- **HyDE** prompts for NVIDIA driver installation, theme selection, SDDM
  setup, and more via plain bash `read`.
- **JaKooLit** (Arch-Hyprland and siblings) uses `whiptail` dialogs for
  Bluetooth, ROG-laptop utils, GTK themes, GDM-vs-SDDM choice, etc.
- **ML4W** runs its own installer flow with multiple choice points.

Rice Cooker is a GUI-only product (terminal is never exposed to the user),
so we cannot simply forward these prompts to the user. We need to answer
them from the catalog, or defer these rices.

For v1, we defer. This issue tracks adding support in a follow-up release.

## Requirements

- No terminal ever reaches the user.
- Prompts must be answered deterministically from the catalog. No fallback
  to "ask the user" — if we can't answer it from catalog, the rice is not
  supported.
- Answers must be tied to a specific rice commit (re-verified when the pin
  is bumped).
- Must handle both bash `read` style (reads from stdin) and
  `whiptail`/`dialog` style (reads from `/dev/tty`, bypasses stdin
  redirection).

## Options

### Option 1 — Stdin answer stream

**Approach.** Catalog declares an array of answers. Rice Cooker writes them
to `install_cmd`'s stdin in order.

```toml
[hyde]
install_cmd   = "./Scripts/install.sh"
answers_stdin = ["y\n", "n\n", "y\n", "y\n"]
```

**Pros.** Trivial to implement (~20 LOC on top of v1). No extra runtime
deps.

**Cons.**
- **Fragile ordering.** If upstream adds a prompt, every subsequent answer
  shifts by one. Silent wrong answers.
- **Doesn't work with whiptail/dialog.** Those read from `/dev/tty`
  directly, ignoring piped stdin. JaKooLit would hang.
- No visibility into which prompt is being answered with which value —
  debugging a misanswer means rerunning and counting prompts.

**Verdict.** Viable only for rices using pure bash `read` with a stable
prompt sequence. Covers HyDE but not JaKooLit.

### Option 2 — Expect-style pattern/response

**Approach.** Run `install_cmd` under a pty (so the child thinks it has a
terminal). Catalog declares pattern-response pairs. Rice Cooker watches the
child's output; when a regex pattern matches, it writes the corresponding
response.

```toml
[jakoolit-arch]
install_cmd       = "./install.sh"
answers_interactive = [
  { pattern = "Install NVIDIA",           response = "n" },
  { pattern = "Install Bluetooth",        response = "y" },
  { pattern = "ASUS ROG utilities",       response = "n" },
  { pattern = "Catppuccin SDDM theme",    response = "y" },
]
```

**Pros.**
- Pattern-matched, so order-independent. If upstream adds a prompt,
  existing answers still find their targets; the new prompt either has a
  declared pattern (works) or doesn't (install hangs, detected in catalog
  CI).
- Works with whiptail/dialog because the child is talking to a pty, not
  stdin.
- Debuggable: when a response is sent, log the triggering pattern and the
  response. Misanswers are traceable.

**Cons.**
- Requires a pty abstraction. In Rust: the `portable-pty` or `expectrl`
  crate (~one extra dep, both mature).
- ~100 LOC for the pattern-matching loop, timeout handling, and
  partial-read buffering.
- Timeout needed per install: if no declared pattern matches within N
  seconds of the child being idle and the child is still alive, assume a
  new prompt exists that the catalog doesn't cover. Abort with a specific
  error code so catalog maintenance knows.

**Verdict.** Strict superset of Option 1. Higher implementation cost but
handles both prompt styles and is robust to upstream changes.

## Recommendation

Implement Option 2. The ~100 LOC cost is acceptable, and Option 1's
fragility would cause production incidents every time upstream adds a
prompt.

## Non-goals for this issue

- User-facing answer overrides. Catalog is the source of truth. If a user
  wants different answers, they fork the catalog (or contribute upstream
  with a variant entry).
- GUI "advanced install" with question forwarding. Explicitly rejected —
  the product requirement is zero terminal exposure. If a prompt can't be
  catalog-answered, the rice is not supported.
- Inferring answers from rice README or install.sh source. Too
  error-prone. Catalog entries for interactive rices are hand-curated and
  CI-tested.

## Work breakdown

1. Add `answers_interactive: Vec<{pattern: String, response: String}>`
   field to catalog schema.
2. Add `interactive = true` support path in `install` command: when set,
   run `install_cmd` via `portable-pty` instead of closing stdin.
3. Implement the pattern-response loop in `src/install/interactive.rs`
   (~100 LOC).
4. Add idle timeout (default 30s since last output, configurable
   per-entry).
5. Catalog CI: for every `interactive = true` entry, test against a pinned
   clean-Arch VM image, assert install completes within timeout.
6. Add HyDE and JaKooLit to the catalog with tested answer sets. Document
   the NVIDIA/Bluetooth/etc. choices made.

## Open questions

- Should we log every pattern match + response to the install log?
  (Leaning yes — it's debug-critical.)
- How do we handle a rice whose prompts include genuinely user-specific
  choices (e.g., "which of your monitors is primary?")? Candidate answer:
  do not support such rices. They are fundamentally incompatible with the
  "catalog answers everything" model.
- Should catalog entries support a `prompt_timeout_seconds` per-pattern
  override, for rices with known long pauses (e.g., compiling something
  before the next prompt)? (Leaning yes.)
