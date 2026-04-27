#!/usr/bin/env bash
# Preview each rice in the catalog; pause for manual inspection; then either
# revert the preview or confirm a real install.
#
# Per rice:
#   1. rice-cooker-backend preview <name>   (launches + verifies, preview deps only)
#   2. parse NDJSON, check for Success
#   3. confirm rice shell process is alive post-preview
#   4. prompt: revert preview, install for real when catalog-supported, or
#      leave active and stop
#   5. after revert/install uninstall, verify state is clean:
#      symlink gone, record gone, current.json gone
#   6. verify clone cache PERSISTS (XDG cache contract)

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BIN=$ROOT/target/release/rice-cooker-backend
CAT=$ROOT/catalog.toml
RENDER_SETTLE=2   # seconds to wait after `preview` returns before hyprctl check
LOG_ROOT=${RICE_COOKER_TEST_LOG_DIR:-/tmp/rice-cooker-catalog-logs/$(date +%Y%m%d-%H%M%S)}
# Run via systemd-run --scope so the backend lands in the user session's
# cgroup tree and polkit recognizes us as a session subject — auth_admin_keep
# should then retain across rice boundaries.
RUN=(systemd-run --user --scope --quiet --collect)

mkdir -p "$LOG_ROOT"
printf 'logs: %s\n' "$LOG_ROOT"

# Rices to preview, in catalog order.
mapfile -t RICES < <(awk '
    /^\[[^][]+\]$/ {
        name = $0
        sub(/^\[/, "", name)
        sub(/\]$/, "", name)
        print name
    }
' "$CAT")

# Stop the user's personal `qs -c clock` shell so this script can
# take the display. Restored at the bottom. Any other quickshell
# instances are left alone.
pkill -xf 'qs -c clock' 2>/dev/null || true
sleep 1

pass=0
fail=0
failed_names=()
stopped_with_active=0

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

safe_log_name() {
    local name=$1
    printf '%s' "${name//[^[:alnum:]._-]/_}"
}

run_logged() {
    local out_log=$1
    local err_log=$2
    shift 2

    : >"$out_log"
    : >"$err_log"
    "$@" > >(tee "$out_log") 2> >(tee "$err_log" >&2)
}

prompt_next() {
    local prompt=${1:-"Press enter for next rice, or type q to stop: "}
    local reply
    if [ -r /dev/tty ]; then
        read -r -p "$prompt" reply </dev/tty
    else
        read -r -p "$prompt" reply
    fi
    case "$reply" in
        q|Q|quit|QUIT|stop|STOP)
            return 1
            ;;
        *)
            return 0
            ;;
    esac
}

prompt_preview_action() {
    local can_install=${1:-0}
    local prompt
    if [ "$can_install" -eq 1 ]; then
        prompt="Press enter/r to revert preview, i to install for real, or q to leave active and stop: "
    else
        prompt="Press enter/r to revert preview, or q to leave active and stop: "
    fi
    local reply
    while true; do
        if [ -r /dev/tty ]; then
            read -r -p "$prompt" reply </dev/tty
        else
            read -r -p "$prompt" reply
        fi
        case "$reply" in
            ""|r|R|revert|REVERT)
                echo "revert"
                return 0
                ;;
            q|Q|quit|QUIT|stop|STOP)
                echo "stop"
                return 0
                ;;
            i|I|install|INSTALL)
                if [ "$can_install" -eq 1 ]; then
                    echo "install"
                    return 0
                fi
                echo "install is not supported for this rice; revert preview or stop" >&2
                ;;
            *)
                if [ "$can_install" -eq 1 ]; then
                    echo "enter, r, i, or q" >&2
                else
                    echo "enter, r, or q" >&2
                fi
                ;;
        esac
    done
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

install_supported_for() {
    local name=$1
    local raw
    raw=$(awk -v n="[$name]" '
        $0 == n { in_block=1; next }
        /^\[/ { in_block=0 }
        in_block && /^install_supported/ {
            sub(/^install_supported *= */, ""); print; exit
        }
    ' "$CAT")
    raw=${raw%%#*}
    raw=${raw%%[[:space:]]*}
    [ "$raw" = "true" ] && echo 1 || echo 0
}

pgrep_count() {
    local pattern=$1
    local out status
    out=$(pgrep -xcf "$pattern" 2>/dev/null)
    status=$?
    case "$status" in
        0|1)
            printf '%s\n' "${out:-0}"
            ;;
        *)
            echo "pgrep failed for pattern: $pattern" >&2
            return "$status"
            ;;
    esac
}

uninstall_active() {
    local name=$1
    local uninstall_log=$2
    local uninstall_err=$3

    printf 'uninstall log: %s\n' "$uninstall_log"
    if ! run_logged "$uninstall_log" "$uninstall_err" "${RUN[@]}" "$BIN" --catalog "$CAT" uninstall; then
        reason=$(ndjson_fail_reason "$uninstall_log")
        printf '%-24s FAIL uninstall: %s\n' "$name" "${reason:-unknown}"
        tail -5 "$uninstall_err"
        fail=$((fail+1)); failed_names+=("$name:uninstall:${reason:-?}")
        return 1
    fi
    if ! ndjson_succeeded "$uninstall_log"; then
        reason=$(ndjson_fail_reason "$uninstall_log")
        printf '%-24s FAIL uninstall-no-success: %s\n' "$name" "${reason:-no-success-event}"
        fail=$((fail+1)); failed_names+=("$name:uninstall-no-success:${reason:-?}")
        return 1
    fi
    return 0
}

teardown_problem_for() {
    local name=$1
    local dst_expanded=$2
    local problem=

    [ -L "$dst_expanded" ] && problem="symlink-still-there;"
    [ -f "$HOME/.local/share/rice-cooker/installs/$name.json" ] && problem="${problem}record-still-there;"
    [ -f "$HOME/.local/share/rice-cooker/installs/current.json" ] && problem="${problem}current-still-there;"
    [ ! -d "$HOME/.cache/rice-cooker/rices/$name" ] && problem="${problem}clone-was-deleted;"

    printf '%s' "$problem"
}

for name in "${RICES[@]}"; do
    printf '\n========== %s ==========\n' "$name"
    log_name=$(safe_log_name "$name")
    preview_log="$LOG_ROOT/$log_name.preview.ndjson"
    preview_err="$LOG_ROOT/$log_name.preview.stderr"
    install_log="$LOG_ROOT/$log_name.install.ndjson"
    install_err="$LOG_ROOT/$log_name.install.stderr"
    uninstall_log="$LOG_ROOT/$log_name.uninstall.ndjson"
    uninstall_err="$LOG_ROOT/$log_name.uninstall.stderr"
    shell_log="$LOG_ROOT/$log_name.quickshell.log"

    # Reset install state but KEEP the clone cache — that's the whole
    # point of the new contract. Tests exercise the cache-hit path.
    rm -f "$HOME/.local/share/rice-cooker/installs/$name.json"
    rm -f "$HOME/.local/share/rice-cooker/installs/current.json"

    # preview — clone + preview deps + symlink + record + kill qs + launch + verify.
    printf 'preview log: %s\n' "$preview_log"
    if ! run_logged "$preview_log" "$preview_err" "${RUN[@]}" "$BIN" --catalog "$CAT" preview "$name"; then
        reason=$(ndjson_fail_reason "$preview_log")
        printf '%-24s FAIL preview: %s\n' "$name" "${reason:-unknown}"
        tail -5 "$preview_err"
        fail=$((fail+1)); failed_names+=("$name:preview:${reason:-?}")
        uninstall_active "$name" "$uninstall_log" "$uninstall_err" || true
        prompt_next || break
        continue
    fi
    if ! ndjson_succeeded "$preview_log"; then
        reason=$(ndjson_fail_reason "$preview_log")
        printf '%-24s FAIL preview-no-success: %s\n' "$name" "${reason:-no-success-event}"
        fail=$((fail+1)); failed_names+=("$name:no-success:${reason:-?}")
        uninstall_active "$name" "$uninstall_log" "$uninstall_err" || true
        prompt_next || break
        continue
    fi

    # Catalog-declared symlink target should exist post-preview.
    dst_expanded=$(symlink_dst_for "$name")
    if [ ! -L "$dst_expanded" ]; then
        echo "SYMLINK MISSING at $dst_expanded"
        run_logged "$uninstall_log" "$uninstall_err" "${RUN[@]}" "$BIN" --catalog "$CAT" uninstall
        fail=$((fail+1)); failed_names+=("$name:symlink-missing")
        prompt_next || break
        continue
    fi

    # Belt-and-suspenders: let the shell settle, then confirm the process is
    # still alive. `preview`'s internal verify handles the hyprctl layer check.
    sleep "$RENDER_SETTLE"
    if ! active_alive=$(pgrep_count "quickshell -c $name"); then
        fail=$((fail+1)); failed_names+=("$name:pgrep")
        uninstall_active "$name" "$uninstall_log" "$uninstall_err" || true
        prompt_next || break
        continue
    fi
    if [ -f "$HOME/.cache/rice-cooker/last-run.log" ]; then
        cp "$HOME/.cache/rice-cooker/last-run.log" "$shell_log"
        printf 'quickshell log: %s\n' "$shell_log"
    fi

    printf '\n%s is active. Inspect it now.\n' "$name"
    can_install=$(install_supported_for "$name")
    if [ "$can_install" -eq 0 ]; then
        printf 'install unavailable: catalog marks this rice preview-only\n'
    fi
    action=$(prompt_preview_action "$can_install")
    if [ "$action" = "stop" ]; then
        stopped_with_active=1
        break
    fi

    if [ "$action" = "install" ]; then
        printf 'install log: %s\n' "$install_log"
        if ! run_logged "$install_log" "$install_err" "${RUN[@]}" "$BIN" --catalog "$CAT" try "$name"; then
            reason=$(ndjson_fail_reason "$install_log")
            printf '%-24s FAIL install: %s\n' "$name" "${reason:-unknown}"
            tail -5 "$install_err"
            fail=$((fail+1)); failed_names+=("$name:install:${reason:-?}")
            uninstall_active "$name" "$uninstall_log" "$uninstall_err" || true
            prompt_next || break
            continue
        fi
        if ! ndjson_succeeded "$install_log"; then
            reason=$(ndjson_fail_reason "$install_log")
            printf '%-24s FAIL install-no-success: %s\n' "$name" "${reason:-no-success-event}"
            fail=$((fail+1)); failed_names+=("$name:install-no-success:${reason:-?}")
            uninstall_active "$name" "$uninstall_log" "$uninstall_err" || true
            prompt_next || break
            continue
        fi

        sleep "$RENDER_SETTLE"
        if ! active_alive=$(pgrep_count "quickshell -c $name"); then
            fail=$((fail+1)); failed_names+=("$name:pgrep")
            uninstall_active "$name" "$uninstall_log" "$uninstall_err" || true
            prompt_next || break
            continue
        fi
        if [ -f "$HOME/.cache/rice-cooker/last-run.log" ]; then
            cp "$HOME/.cache/rice-cooker/last-run.log" "$shell_log"
            printf 'quickshell log: %s\n' "$shell_log"
        fi

        printf '\n%s is installed. Inspect it now.\n' "$name"
        if ! prompt_next "Press enter to uninstall installed $name, or type q to leave it active and stop: "; then
            stopped_with_active=1
            break
        fi
    fi

    # uninstall — should kill qs, remove deps/symlink/record, and (since
    # we had no captured original shell going in) skip the replay step.
    uninstall_active "$name" "$uninstall_log" "$uninstall_err" || {
        prompt_next || break
        continue
    }

    # Post-uninstall state:
    #   - symlink   gone
    #   - record    gone
    #   - current   gone
    #   - clone     PRESENT (XDG cache, intentionally persisted)
    leftover=$(teardown_problem_for "$name" "$dst_expanded")

    if [ -n "$leftover" ]; then
        printf '%-24s FAIL uninstall-left-behind: %s\n' "$name" "$leftover"
        fail=$((fail+1)); failed_names+=("$name:leftover")
    elif [ "$active_alive" -lt 1 ]; then
        printf '%-24s FAIL shell-not-running-post-preview\n' "$name"
        fail=$((fail+1)); failed_names+=("$name:no-shell")
    else
        if [ "$action" = "install" ]; then
            printf '%-24s PASS installed then reverted\n' "$name"
        else
            printf '%-24s PASS preview reverted\n' "$name"
        fi
        pass=$((pass+1))
    fi

    prompt_next || break
done

echo
echo "========================"
printf '  pass: %d    fail: %d\n' "$pass" "$fail"
if [ "${#failed_names[@]}" -gt 0 ]; then
    echo "  failures:"
    for f in "${failed_names[@]}"; do echo "    $f"; done
fi
echo "========================"

if [ "$stopped_with_active" -eq 0 ]; then
    # Restore user's pre-test shell.
    setsid qs -c clock >/dev/null 2>&1 &
    disown
else
    echo "left active rice running; not restoring qs -c clock"
fi
