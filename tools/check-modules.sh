#!/usr/bin/env bash
# Every module next to this repository, the way check.sh checks the shell: no warnings, unit tests,
# i18n, every state rendered in both looks and three languages, scripts parse.
# Usage: tools/check-modules.sh [yami-files …]    the modules live in YAMI_MODULES_DIR (../yami-modules)
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
# shellcheck source=/dev/null
[[ -f $MODS/env.sh ]] && . "$MODS/env.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
fail=0
bad() { printf '\e[31m  ✗ %s\e[0m\n' "$1"; fail=1; }
ok() { printf '\e[32m  ✓ %s\e[0m\n' "$1"; }
printf '{"preset":"plain"}\n' >"$TMP/plain.json"
printf '{"preset":"yami","palette":"ash"}\n' >"$TMP/yami.json"
list=("$@")
((${#list[@]})) || list=(yami-px yami-files yami-view yami-play yami-shot yami-agents yami-greet)
for m in "${list[@]}"; do
  d="$MODS/$m"
  [[ -d $d ]] || { bad "$m: no $d"; continue; }
  printf '\e[1m── %s\e[0m\n' "$m"
  if out="$(cd "$d" && cargo build --release --message-format short 2>&1)"; then
    if grep -q 'warning:' <<<"$out"; then
      grep 'warning:' <<<"$out" | grep -v 'generated [0-9]* warning' | head
      bad "warnings"
    else
      ok "build"
    fi
  else
    grep -E '^(error|.*: error)' <<<"$out" | head
    bad "build"
    continue
  fi
  (cd "$d" && cargo test --release --quiet >"$TMP/test.log" 2>&1) && ok "tests" || { tail -20 "$TMP/test.log"; bad "tests"; }
  for f in "$d"/*.sh; do [[ -f $f ]] && { bash -n "$f" || bad "syntax: $f"; }; done
  [[ $m == yami-px ]] && continue
  python3 -I "$ROOT/tools/check-i18n.py" "$d/src" >/dev/null && ok "i18n" || { python3 -I "$ROOT/tools/check-i18n.py" "$d/src" | head; bad "i18n"; }
  exe="$d/target/release/$m"
  for look in plain yami; do
    for lang in en ru pt-BR; do
      if YAMI_STYLE="$TMP/$look.json" "$exe" shot --lang "$lang" -o "$TMP/$m-$look-$lang" >"$TMP/shot.log" 2>&1; then :; else
        tail -5 "$TMP/shot.log"
        bad "shot: $look, $lang"
      fi
    done
  done
  ok "renders: plain and yami, en ru pt-BR"
done
echo
if ((fail)); then
  printf '\e[31mcheck-modules: FAILED\e[0m\n'
  exit 1
fi
printf '\e[32mcheck-modules: all good\e[0m\n'
