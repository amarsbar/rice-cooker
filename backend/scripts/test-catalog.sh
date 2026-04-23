#!/usr/bin/env bash
# Try each rice in the catalog; uninstall it; check a clean teardown.
# Skips caelestia (separately verified) and dotfiles-hyprland (known broken).
#
# Per rice:
#   1. rice-cooker-backend try <name>   (installs + launches + verifies)
#   2. parse NDJSON, check for Success
#   3. verify hyprctl layer-shell surfaces (belt-and-suspenders beyond
#      what `try` already checked internally)
#   4. rice-cooker-backend uninstall
#   5. parse NDJSON, check for Success
#   6. verify state is clean: symlink gone, record gone, current.json gone
#   7. verify clone cache PERSISTS (new XDG-cache contract)
# Passes summarize at the end.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN=$ROOT/target/release/rice-cooker-backend
CAT=$ROOT/catalog.toml
RENDER_SETTLE=2   # seconds to wait after `try` returns before hyprctl check

# Rices to try, in order. Skip caelestia (tested separately) + dotfiles-hyprland (known bad).
RICES=(
    noctalia dms end-4 Ambxst iNiR nucleus NibrasShell
    linux-retroism Zaphkiel eqsh whisker dhrruvsharma-shell Moonveil
)

# Stop the user's personal `qs -c clock` shell so this script can
# take the display. Restored at the bottom. Any other quickshell
# instances are left alone.
pkill -xf 'qs -c clock' 2>/dev/null || true
sleep 1

pass=0
fail=0
declare -a failed_names

# Returns 0 if the NDJSON at $1 contains a line with "type":"success" and
# no "type":"fail". Otherwise 1.
ndjson_succeeded() {
    local log=$1
    grep -q '"type":"success"' "$log" || return 1
    ! grep -q '"type":"fail"' "$log"
}

# Print the fail reason (if any) from an NDJSON log, or empty string.
ndjson_fail_reason() {
    local log=$1
    grep '"type":"fail"' "$log" | head -1 | sed 's/.*"reason":"\([^"]*\)".*/\1/'
}

# Parse symlink_dst for a rice from catalog.toml (awk on [name] table),
# then expand leading ~ to $HOME.
symlink_dst_for() {
    local name=$1
    local raw
    raw=$(awk -v n="[$name]" '
        $0 == n { in_block=1; next }
        /^\[/ { in_block=0 }
        in_block && /^symlink_dst/ {
            sub(/^symlink_dst *= *"/, ""); sub(/".*/, ""); print; exit
        }
    ' "$CAT")
    echo "${raw/#\~/$HOME}"
}

# Count hyprctl layer-shell surfaces owned by pids matching `quickshell -c <name>`.
# python3 defends against hyprctl/json failures — empty output becomes 0.
count_layers_for() {
    local name=$1
    local pids
    pids=$(pgrep -xf "quickshell -c $name" 2>/dev/null | tr '\n' ' ')
    if [ -z "$pids" ]; then
        echo 0
        return
    fi
    PIDS="$pids" hyprctl layers -j 2>/dev/null | python3 -c '
import json, os, sys
try:
    root = json.load(sys.stdin)
except Exception:
    print(0); sys.exit(0)
pids = {int(p) for p in os.environ.get("PIDS", "").split() if p.isdigit()}
count = 0
for mon in root.values():
    for arr in mon.get("levels", {}).values():
        for layer in arr:
            if layer.get("pid") in pids:
                count += 1
print(count)
' 2>/dev/null || echo 0
}

for name in "${RICES[@]}"; do
    printf '\n========== %s ==========\n' "$name"

    # Reset install state but KEEP the clone cache — that's the whole
    # point of the new contract. Tests exercise the cache-hit path.
    rm -f "$HOME/.local/share/rice-cooker/installs/$name.json"
    rm -f "$HOME/.local/share/rice-cooker/installs/current.json"

    # try — full pipeline: clone + deps + symlink + record + kill qs + launch + verify.
    if ! "$BIN" --catalog "$CAT" try "$name" >/tmp/rc-try.log 2>/tmp/rc-try.err; then
        reason=$(ndjson_fail_reason /tmp/rc-try.log)
        printf '%-24s FAIL try: %s\n' "$name" "${reason:-unknown}"
        tail -5 /tmp/rc-try.err
        fail=$((fail+1)); failed_names+=("$name:try:${reason:-?}")
        continue
    fi
    if ! ndjson_succeeded /tmp/rc-try.log; then
        reason=$(ndjson_fail_reason /tmp/rc-try.log)
        printf '%-24s FAIL try-no-success: %s\n' "$name" "${reason:-no-success-event}"
        fail=$((fail+1)); failed_names+=("$name:no-success:${reason:-?}")
        continue
    fi

    # Catalog-declared symlink target should exist post-try.
    dst_expanded=$(symlink_dst_for "$name")
    if [ ! -L "$dst_expanded" ]; then
        echo "SYMLINK MISSING at $dst_expanded"
        "$BIN" --catalog "$CAT" uninstall >/dev/null 2>&1
        fail=$((fail+1)); failed_names+=("$name:symlink-missing")
        continue
    fi

    # Belt-and-suspenders: let the shell settle, then re-check hyprctl.
    sleep "$RENDER_SETTLE"
    layers=$(count_layers_for "$name")
    layers=${layers:-0}

    # uninstall — should kill qs, remove deps/symlink/record, and (since
    # we had no captured original shell going in) skip the replay step.
    if ! "$BIN" --catalog "$CAT" uninstall >/tmp/rc-uninstall.log 2>/tmp/rc-uninstall.err; then
        reason=$(ndjson_fail_reason /tmp/rc-uninstall.log)
        printf '%-24s FAIL uninstall: %s\n' "$name" "${reason:-unknown}"
        tail -5 /tmp/rc-uninstall.err
        fail=$((fail+1)); failed_names+=("$name:uninstall:${reason:-?}")
        continue
    fi
    if ! ndjson_succeeded /tmp/rc-uninstall.log; then
        reason=$(ndjson_fail_reason /tmp/rc-uninstall.log)
        printf '%-24s FAIL uninstall-no-success: %s\n' "$name" "${reason:-no-success-event}"
        fail=$((fail+1)); failed_names+=("$name:uninstall-no-success:${reason:-?}")
        continue
    fi

    # Post-uninstall state:
    #   - symlink   gone
    #   - record    gone
    #   - current   gone
    #   - clone     PRESENT (XDG cache, intentionally persisted)
    leftover=
    [ -L "$dst_expanded" ] && leftover="symlink-still-there;"
    [ -f "$HOME/.local/share/rice-cooker/installs/$name.json" ] && leftover="${leftover}record-still-there;"
    [ -f "$HOME/.local/share/rice-cooker/installs/current.json" ] && leftover="${leftover}current-still-there;"

    clone_missing=
    if [ ! -d "$HOME/.cache/rice-cooker/rices/$name" ]; then
        clone_missing="clone-was-deleted;"
    fi

    if [ -n "$leftover" ]; then
        printf '%-24s FAIL uninstall-left-behind: %s\n' "$name" "$leftover"
        fail=$((fail+1)); failed_names+=("$name:leftover")
    elif [ -n "$clone_missing" ]; then
        # New contract: uninstall keeps the clone. If it's gone, someone
        # broke the XDG cache guarantee.
        printf '%-24s FAIL %s\n' "$name" "$clone_missing"
        fail=$((fail+1)); failed_names+=("$name:clone-wrongly-deleted")
    elif [ "$layers" -lt 1 ]; then
        printf '%-24s FAIL zero-layer-surfaces (try said ok but hyprctl saw nothing)\n' "$name"
        fail=$((fail+1)); failed_names+=("$name:no-layers")
    else
        printf '%-24s PASS %d layer-shell surface(s), clone cached\n' "$name" "$layers"
        pass=$((pass+1))
    fi
done

echo
echo "========================"
printf '  pass: %d    fail: %d\n' "$pass" "$fail"
if [ "${#failed_names[@]}" -gt 0 ]; then
    echo "  failures:"
    for f in "${failed_names[@]}"; do echo "    $f"; done
fi
echo "========================"

# Restore user's pre-test shell.
setsid qs -c clock >/dev/null 2>&1 &
disown
