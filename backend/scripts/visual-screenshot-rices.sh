#!/usr/bin/env bash
# Live visual confirmation: for each of the 15 catalog rices, apply it,
# grim a full-screen screenshot to backend/screenshots/<id>.png, short
# pause so the compositor has time to render, then move on. Final `exit`
# restores whatever shell was running before the script started.
#
# Run from backend/ with grim available (`sudo pacman -S grim` first
# on Arch-likes) and a release-built binary
# (`cargo build --release`).
#
# Uses `apply` (not `install`) because apply owns the kill-and-replace
# dance that lets N shells take over the screen in sequence, and `exit`
# restores the user's pre-script shell cleanly.
set -euo pipefail

cd "$(dirname "$0")/.."
BIN="$PWD/target/release/rice-cooker-backend"
CATALOG="$PWD/catalog.toml"
SHOTS="$PWD/screenshots"
RENDER_WAIT=4       # seconds to wait for the rice's shell to render before grim
POST_SHOT_WAIT=1    # breathing room after grim before the next apply kills qs

if [[ ! -x "$BIN" ]]; then
    echo "release build missing: cargo build --release" >&2
    exit 1
fi
if ! command -v grim >/dev/null 2>&1; then
    echo "grim not installed. Run: sudo pacman -S grim" >&2
    exit 1
fi

mkdir -p "$SHOTS"

# Extract `[rice_name]` headers and their repo = / entry = / commit =
# from catalog.toml. Uses awk so the script doesn't need a toml parser.
mapfile -t ENTRIES < <(awk '
    /^\[[A-Za-z0-9_.-]+\]$/ {
        if (name != "") print name "|" repo
        name = substr($0, 2, length($0)-2)
        repo = ""
    }
    /^repo = "/ {
        sub(/^repo = "/, ""); sub(/".*/, "")
        repo = $0
    }
    END { if (name != "") print name "|" repo }
' "$CATALOG")

echo "=========================================="
echo "  live screenshot confirmation: ${#ENTRIES[@]} rices"
echo "  ${RENDER_WAIT}s render wait, ${POST_SHOT_WAIT}s post-shot breather"
echo "  shots → ${SHOTS}/"
echo "=========================================="

for entry in "${ENTRIES[@]}"; do
    name="${entry%%|*}"
    repo="${entry##*|}"
    printf '\n--- %s (%s) ---\n' "$name" "$repo"
    # apply expects --entry; shell.qml + resolve_entry's walk fallback
    # handles the layout variance across the catalog.
    # Capture exit code explicitly: set -e would bail the whole script
    # on a single failing apply, and `|| true` would mask the failure
    # and still run grim against a black screen (or the previous rice).
    apply_rc=0
    "$BIN" apply --name "$name" --repo "$repo" --entry "shell.qml" 2>&1 | tail -4
    apply_rc=${PIPESTATUS[0]}
    if [[ $apply_rc -ne 0 ]]; then
        echo "apply failed for $name (exit $apply_rc); skipping screenshot"
        continue
    fi
    sleep "$RENDER_WAIT"
    grim "${SHOTS}/${name}.png" || echo "grim failed for $name; skipping shot"
    sleep "$POST_SHOT_WAIT"
done

echo
echo "=========================================="
echo "  restoring pre-script shell via \`exit\`"
echo "=========================================="
"$BIN" exit 2>&1 | tail -3 || true

echo
echo "screenshots:"
ls -la "$SHOTS" || true
