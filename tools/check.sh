#!/usr/bin/env bash
# One command before every commit: build without warnings, i18n (static + every panel rendered
# in English), the English and Russian docs in step, shell syntax, niri config,
# installer dry run in a throwaway HOME.
# Touches nothing outside the repo and the temp dir. Usage: tools/check.sh [--fast]
#   --fast  skip the installer dry run
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# пока модулей нет на GitHub: yami-px берётся из соседней папки (env.sh указывает git туда)
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
# shellcheck source=/dev/null
[[ -f $MODS/env.sh ]] && . "$MODS/env.sh"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
FAST=0
[[ ${1:-} == --fast ]] && FAST=1
fail=0
step() { printf '\e[1m── %s\e[0m\n' "$1"; }
bad() { printf '\e[31m✗ %s\e[0m\n' "$1"; fail=1; }
ok() { printf '\e[32m✓ %s\e[0m\n' "$1"; }

step "build (release, warnings are errors)"
out="$(cargo build --release --locked --workspace --message-format short --manifest-path "$ROOT/shell/Cargo.toml" 2>&1)"
if [[ $? -ne 0 ]]; then
  printf '%s\n' "$out" | grep -E '^(error|.*: error)' | head -20
  bad "build failed"
elif grep -q 'warning:' <<<"$out"; then
  grep 'warning:' <<<"$out" | grep -v 'generated [0-9]* warning' | head -20
  bad "build has warnings"
else
  ok "build"
fi
BIN="$ROOT/shell/target/release/yami-shell"

step "versions"
want="$(cat "$ROOT/VERSION")"
vbad=0
for f in "$ROOT"/shell/Cargo.toml "$ROOT"/shell/*/Cargo.toml; do
  [[ -f $f ]] || continue
  v="$(sed -n '0,/^version = "\(.*\)"/s//\1/p' "$f")"
  [[ $v == "$want" ]] || { echo "${f#"$ROOT"/}: $v, VERSION: $want"; vbad=1; }
done
((vbad)) && bad "crate versions differ from VERSION (tools/release.sh sets them)" || ok "every crate is $want"

step "i18n"
DEV="$ROOT/shell/target/release/yami-dev"
"$DEV" check-i18n || bad "static i18n"
# README, CHANGELOG, docs/* and their .ru.md twins: one skeleton, links, anchors
"$DEV" check-docs && ok "docs: English and Russian in step" || bad "docs EN/RU (yami-dev check-docs)"
if [[ -x $BIN ]]; then
  # every panel drawn offscreen in English: any Russian text left on screen fails
  "$BIN" shot --defaults --lang en -o "$TMP/en" | grep untranslated
  [[ ${PIPESTATUS[0]} -eq 0 ]] && ok "shot --lang en: no Russian left" || bad "shot --lang en"
  "$BIN" shot --defaults --lang ru -o "$TMP/ru" >/dev/null || bad "shot --lang ru (render crashed?)"
fi
SETUP="$ROOT/shell/target/release/yami-setup"
if [[ -x $SETUP ]]; then
  "$SETUP" shot --lang en -o "$TMP/sen" >/dev/null && ok "yami-setup: every page renders" || bad "yami-setup shot"
  "$SETUP" shot --lang en --plain -o "$TMP/sep" >/dev/null && ok "yami-setup: the neutral look renders" || bad "yami-setup shot --plain"
fi
# the shell in Brazilian Portuguese: nothing left in Russian or English from the catalogs
"$BIN" shot --lang pt-BR -o "$TMP/pt" --defaults | grep untranslated
[[ ${PIPESTATUS[0]} -eq 0 ]] && ok "yami-shell shot --lang pt-BR" || bad "yami-shell shot --lang pt-BR"
step "unit tests"
cargo test --release --locked --workspace --quiet --manifest-path "$ROOT/shell/Cargo.toml" >"$TMP/test.log" 2>&1 && ok "cargo test" || { tail -20 "$TMP/test.log"; bad "cargo test"; }
# a package manager's build (Nix, distro packages): the updater is off (docs/PACKAGING.md)
cargo test --release --locked -p yami-shell --features package-managed --quiet --manifest-path "$ROOT/shell/Cargo.toml" package_manager_owns_updates >"$TMP/managed.log" 2>&1 && ok "cargo test (package-managed)" || { tail -20 "$TMP/managed.log"; bad "cargo test --features package-managed"; }

step "scripts"
for f in "$ROOT"/install.sh "$ROOT"/installer/*.sh "$ROOT"/tools/*.sh; do
  bash -n "$f" || bad "syntax: ${f#"$ROOT"/}"
done
if command -v shellcheck >/dev/null; then
  shellcheck -S error -x "$ROOT"/install.sh "$ROOT"/installer/*.sh "$ROOT"/tools/*.sh || bad "shellcheck"
fi
ok "scripts parsed"

# checks of a local checkout that are not part of the repository
if [[ -x $ROOT/tools/check.local.sh ]]; then
  step "local"
  "$ROOT/tools/check.local.sh" || bad "tools/check.local.sh"
fi

step "niri config"
if command -v niri >/dev/null; then
  niri validate -c "$ROOT/config/niri/config.kdl" >/dev/null 2>&1 && ok "config.kdl valid" || bad "niri validate config.kdl"
else
  echo "niri not installed, skipped"
fi

if ((!FAST)); then
  step "installer dry run (throwaway HOME)"
  mkdir -p "$TMP/home"
  # a first install, the same with root (the plan only: --dry-run never asks for it), an update
  for args in "--no-sudo" "" "--update" "--full" "--apply files\ shot\ agents" "--lang pt-BR --no-sudo"; do
    eval "argv=($args)"
    if HOME="$TMP/home" NIRI_SOCKET='' XDG_CONFIG_HOME='' XDG_CACHE_HOME='' XDG_STATE_HOME='' XDG_DATA_HOME='' \
      timeout 60 "$ROOT/install.sh" --yes --dry-run --no-anim --lang en "${argv[@]}" >"$TMP/dry.log" 2>&1; then
      ok "install.sh --dry-run $args"
    else
      tail -20 "$TMP/dry.log"
      bad "install.sh --dry-run $args"
    fi
  done
fi

echo
if ((fail)); then
  printf '\e[31mcheck: FAILED\e[0m\n'
  exit 1
fi
printf '\e[32mcheck: all good\e[0m\n'
