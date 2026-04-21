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
    /^\[[A-Za-z0-9_.-]+\]$/ {
        if (name != "") print name "|" repo
        name = substr($0, 2, length($0)-2)
        repo = ""
    }
    /^repo = "/ { sub(/^repo = "/, ""); sub(/".*/, ""); repo = $0 }
    END { if (name != "") print name "|" repo }
' "$CATALOG")

# Filter to requested rices if args given.
if (( $# > 0 )); then
    declare -A want
    for a in "$@"; do want["$a"]=1; done
    filtered=()
    for e in "${ENTRIES[@]}"; do
        name="${e%%|*}"
        [[ -n "${want[$name]:-}" ]] && filtered+=("$e")
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
    type=$(echo "$final" | grep -oE '"type":"[a-z_]+"' | head -1 | cut -d'"' -f4)
    case "$type" in
        success) printf '%-24s ✓ success\n' "$name"; pass=$((pass+1)) ;;
        fail)
            reason=$(echo "$final" | grep -oE '"reason":"[^"]+"' | head -1 | cut -d'"' -f4)
            stage=$(echo "$final" | grep -oE '"stage":"[^"]+"' | head -1 | cut -d'"' -f4)
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
