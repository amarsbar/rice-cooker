#!/usr/bin/env bash
# Fast-iteration verify: no screenshots, no render waits, just run apply
# for each rice and print the final event. Used to shake out verify-stage
# bugs without the 15×5s screenshot loop.
#
# Usage: bash scripts/visual-verify-only.sh [rice_name ...]
#   No args → all 15 rices in catalog.toml order.
#   Args   → only those rices, in the given order.
set -euo pipefail

cd "$(dirname "$0")/.."
BIN="$PWD/target/release/rice-cooker-backend"
CATALOG="$PWD/catalog.toml"

[[ -x "$BIN" ]] || { echo "build the release binary first: cargo build --release" >&2; exit 1; }

mapfile -t ENTRIES < <(awk '
    # Top-level [name] only — nested [name.entry] tables would
    # otherwise emit spurious ENTRIES with empty repo.
    /^\[[A-Za-z0-9_-]+\]$/ {
        if (name != "") print name "|" repo
        name = substr($0, 2, length($0)-2)
        repo = ""
    }
    /^repo = "/ { sub(/^repo = "/, ""); sub(/".*/, ""); repo = $0 }
    END { if (name != "") print name "|" repo }
' "$CATALOG")

# Filter to requested rices if args given. Iterate CLI args in the
# user's given order (not catalog order) so `... dms caelestia` runs
# dms first, caelestia second — matches the mental model of "run these
# three, in this order".
if (( $# > 0 )); then
    declare -A by_name
    for e in "${ENTRIES[@]}"; do
        by_name["${e%%|*}"]="$e"
    done
    filtered=()
    for a in "$@"; do
        if [[ -n "${by_name[$a]:-}" ]]; then
            filtered+=("${by_name[$a]}")
        else
            echo "unknown rice: $a" >&2
            exit 1
        fi
    done
    ENTRIES=("${filtered[@]}")
fi

pass=0
fail=0
declare -a fail_names
for entry in "${ENTRIES[@]}"; do
    name="${entry%%|*}"
    repo="${entry##*|}"
    out=$("$BIN" apply --name "$name" --repo "$repo" --entry "shell.qml" 2>&1 || true)
    final=$(echo "$out" | tail -1)
    # `|| true` on each grep chain: under `set -euo pipefail`, a pipeline
    # whose grep finds no match returns 1, which kills the script before
    # the case below runs. We want "no match" to produce an empty string
    # and fall into the `*)` unknown branch, not abort the whole run.
    type=$(echo "$final" | grep -oE '"type":"[a-z_]+"' | head -1 | cut -d'"' -f4 || true)
    case "$type" in
        success) printf '%-24s ✓ success\n' "$name"; pass=$((pass+1)) ;;
        fail)
            reason=$(echo "$final" | grep -oE '"reason":"[^"]+"' | head -1 | cut -d'"' -f4 || true)
            stage=$(echo "$final" | grep -oE '"stage":"[^"]+"' | head -1 | cut -d'"' -f4 || true)
            printf '%-24s ✗ fail  %s/%s\n' "$name" "$stage" "$reason"
            fail=$((fail+1)); fail_names+=("$name")
            ;;
        *) printf '%-24s ? unknown  %s\n' "$name" "$final"; fail=$((fail+1)); fail_names+=("$name") ;;
    esac
done

echo ""
echo "pass: $pass / ${#ENTRIES[@]}    fail: $fail"
if (( fail > 0 )); then echo "fail list: ${fail_names[*]}"; fi

# restore pre-script shell.
"$BIN" exit >/dev/null 2>&1 || true
