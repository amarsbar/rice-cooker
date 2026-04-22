#!/usr/bin/env bash
# Install + verify + uninstall each rice in the catalog.
# Skips caelestia (separately verified) and dotfiles-hyprland (known broken).
#
# Per rice:
#   1. rice-cooker install <name>
#   2. verify symlink exists at catalog.symlink_dst
#   3. launch `quickshell -c <name>`, wait 5s for render
#   4. check process alive + hyprctl layer-shell surfaces exist
#   5. kill quickshell
#   6. rice-cooker uninstall
#   7. verify symlink + clone + record all gone
# Passes summarize at the end.

set -u

ROOT=/home/user/Documents/Code/rice-cooker
BIN=$ROOT/backend/target/release/rice-cooker-backend
CAT=$ROOT/backend/catalog.toml
RENDER_WAIT=5

# Rices to test, in order. Skip caelestia (tested) + dotfiles-hyprland (known bad).
RICES=(
    noctalia dms end-4 Ambxst iNiR nucleus NibrasShell
    linux-retroism Zaphkiel eqsh whisker dhrruvsharma-shell Moonveil
)

# Stop whatever quickshell is running so we can take over.
pkill -xf 'qs -c clock' 2>/dev/null || true
sleep 1

pass=0
fail=0
declare -a failed_names

for name in "${RICES[@]}"; do
    printf '\n========== %s ==========\n' "$name"

    # Wipe any prior state for this rice.
    rm -rf "$HOME/.cache/rice-cooker/rices/$name"
    rm -f "$HOME/.local/share/rice-cooker/installs/$name.json"
    rm -f "$HOME/.local/share/rice-cooker/installs/current.json"

    # Install.
    if ! "$BIN" --catalog "$CAT" install "$name" >/tmp/rc-install.log 2>&1; then
        echo "INSTALL FAILED"
        tail -5 /tmp/rc-install.log
        fail=$((fail+1)); failed_names+=("$name:install")
        continue
    fi

    # Parse dst from catalog (portable-ish awk on [name] table).
    dst=$(awk -v n="[$name]" '
        $0 == n { in_block=1; next }
        /^\[/ { in_block=0 }
        in_block && /^symlink_dst/ {
            sub(/^symlink_dst *= *"/, ""); sub(/".*/, ""); print; exit
        }
    ' "$CAT")
    # Expand ~ to $HOME.
    dst_expanded="${dst/#\~/$HOME}"

    if [ ! -L "$dst_expanded" ]; then
        echo "SYMLINK MISSING at $dst_expanded"
        "$BIN" --catalog "$CAT" uninstall >/dev/null 2>&1
        fail=$((fail+1)); failed_names+=("$name:symlink-missing")
        continue
    fi

    # Launch quickshell and watch.
    pkill -xf "quickshell -c $name" 2>/dev/null || true
    sleep 0.5
    setsid quickshell -c "$name" >/tmp/rc-shell.log 2>&1 &
    shell_pid=$!
    disown
    sleep "$RENDER_WAIT"

    alive=no
    if kill -0 "$shell_pid" 2>/dev/null; then alive=yes; fi
    # Count caelestia-style layer-shell surfaces for this rice's quickshell.
    layers=$(hyprctl layers -j 2>/dev/null \
        | python3 -c "
import json, sys
root = json.load(sys.stdin)
pid = $shell_pid
count = 0
for mon in root.values():
    for level_arr in mon.get('levels', {}).values():
        for l in level_arr:
            if l.get('pid') == pid:
                count += 1
print(count)
")
    err_snippet=$(grep -iE 'ERROR|Failed' /tmp/rc-shell.log | head -2 | tr '\n' '; ')

    # Kill the shell before uninstall so uninstall can clean the clone.
    kill -TERM "$shell_pid" 2>/dev/null || true
    sleep 1
    pkill -xf "quickshell -c $name" 2>/dev/null || true
    sleep 1

    # Uninstall.
    if ! "$BIN" --catalog "$CAT" uninstall >/tmp/rc-uninstall.log 2>&1; then
        echo "UNINSTALL FAILED"
        tail -5 /tmp/rc-uninstall.log
        fail=$((fail+1)); failed_names+=("$name:uninstall")
        continue
    fi

    # Verify clean state.
    leftover=
    [ -L "$dst_expanded" ] && leftover="symlink-still-there;"
    [ -d "$HOME/.cache/rice-cooker/rices/$name" ] && leftover="${leftover}clone-still-there;"
    [ -f "$HOME/.local/share/rice-cooker/installs/$name.json" ] && leftover="${leftover}record-still-there;"
    [ -f "$HOME/.local/share/rice-cooker/installs/current.json" ] && leftover="${leftover}current-still-there;"

    # Classify.
    if [ -n "$leftover" ]; then
        printf '%-24s FAIL uninstall-left-behind: %s\n' "$name" "$leftover"
        fail=$((fail+1)); failed_names+=("$name:leftover")
    elif [ "$alive" = no ]; then
        printf '%-24s FAIL shell-died-during-render-wait  err=%s\n' "$name" "$err_snippet"
        fail=$((fail+1)); failed_names+=("$name:died")
    elif [ "$layers" -lt 1 ]; then
        printf '%-24s FAIL zero-layer-surfaces (alive but no UI)  err=%s\n' "$name" "$err_snippet"
        fail=$((fail+1)); failed_names+=("$name:no-layers")
    else
        printf '%-24s PASS %d layer-shell surface(s)\n' "$name" "$layers"
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
