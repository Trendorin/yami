#!/usr/bin/env bash
# Install and update scenarios, end to end and with no network: a fake GitHub of bare repositories with
# signed tags, a throwaway HOME for every scenario, real builds and the real updater (`yami update
# apply`). Nothing reaches the running session: env -i, its own XDG_RUNTIME_DIR, no D-Bus and no
# NIRI_SOCKET; sudo, systemctl, pkill and the like are stubs that write down what was asked of them
# (root commands are never run, so the root half of the login screen is checked by what it was asked).
# The sources are this working tree and the modules next to it, edits included.
#   tools/test-updates.sh [--list] [scenario …]     all of them by default; logs in $YAMI_TEST_DIR/logs
# yami-niri is a stub (a real build is 5–25 minutes and 4 GB); needs a packaged niri for `niri validate`.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
WORK="${YAMI_TEST_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/yami-test}"
GH="$WORK/github/Trendorin"
SRC="$WORK/work"
SHIMS="$WORK/shims"
LOGS="$WORK/logs"
H="$WORK/home"
REAL_HOME="$HOME"
MODULES=(yami-files yami-view yami-play yami-shot yami-agents yami-greet yami-niri)
BINS=(yami-shell yami-files yami-view yami-play yami-shot yami-agents)
PASS=0 FAIL=0 FAILED_NAMES=()

say() { printf '\e[1m── %s\e[0m\n' "$*"; }
ok() { PASS=$((PASS + 1)); printf '  \e[32m✓\e[0m %s\n' "$1"; }
no() { FAIL=$((FAIL + 1)); FAILED_NAMES+=("$SCEN: $1"); printf '  \e[31m✗ %s\e[0m\n' "$1"; [[ -n ${2:-} ]] && printf '    %s\n' "$2"; }
# check "what" command…
check() { local d=$1; shift; if "$@" >/dev/null 2>&1; then ok "$d"; else no "$d" "$*"; fi; }
nocheck() { local d=$1; shift; if "$@" >/dev/null 2>&1; then no "$d" "$*"; else ok "$d"; fi; }
exists() { [[ -e $1 ]]; }
missing() { [[ ! -e $1 ]]; }
contains() { grep -qF -- "$2" "$1"; }

# ── the fake world ──────────────────────────────────────────────────────────
make_shims() {
  mkdir -p "$SHIMS"
  local c
  for c in systemctl busctl pkill gsettings notify-send update-desktop-database gtk-update-icon-cache; do
    printf '#!/bin/sh\necho "%s $*" >>"$HOME/calls.log"\nexit 0\n' "$c" >"$SHIMS/$c"
  done
  # not active, not running: nothing to restart
  printf '#!/bin/sh\necho "systemctl $*" >>"$HOME/calls.log"\ncase "$*" in *is-active*|*is-enabled*) exit 1;; esac\nexit 0\n' >"$SHIMS/systemctl"
  printf '#!/bin/sh\necho "pgrep $*" >>"$HOME/calls.log"\nexit 1\n' >"$SHIMS/pgrep"
  printf '#!/bin/sh\nexit 0\n' >"$SHIMS/greetd"
  # no network in the tests: the counter is recorded, anything else fails as if offline
  cat >"$SHIMS/curl" <<'SH'
#!/bin/sh
echo "curl $*" >>"$HOME/calls.log"
# the installer's own «is the network up» check; everything else is offline
case "$*" in *yami-installs.txt* | *" https://github.com") exit 0 ;; esac
exit 22
SH
  # root is never used: what would run as root is written down. The installer's root shell (a loop
  # reading commands) is answered «done» for each command
  cat >"$SHIMS/sudo" <<'SH'
#!/bin/bash
echo "sudo $*" >>"$HOME/root.log"
args=("$@")
[[ ${args[0]:-} == -A ]] && { args=("${args[@]:1}"); echo "askpass" >>"$HOME/root.log"; }
[[ ${args[0]:-} == -k || ${args[0]:-} == -n ]] && exit 0
if [[ ${args[0]:-} == bash && ${args[1]:-} == -c ]]; then
  while IFS= read -r p && IFS= read -r c; do echo "root: $c" >>"$HOME/root.log"; echo 0; done
  exit 0
fi
exit 0
SH
  chmod +x "$SHIMS"/*
}

# every file of a working tree that git would see, edits included
snap_tree() { # from to
  mkdir -p "$2"
  (
    cd "$1" || exit 1
    git ls-files -z --cached --others --exclude-standard | while IFS= read -r -d '' f; do
      [[ -e $f || -L $f ]] || continue
      mkdir -p "$2/$(dirname "$f")" && cp -a -- "$f" "$2/$f"
    done
  )
}
g() { GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t git -c gpg.format=ssh -c "user.signingkey=$WORK/key.pub" -c commit.gpgsign=false "$@"; }
commit_tag() { # dir tag [key]
  g -C "$1" add -A && g -C "$1" commit -q -m "$2" --allow-empty && g -C "$1" tag -s -m "$2" "$2" ${3:+-c "user.signingkey=$3"}
}
push() { g -C "$SRC/$1" push -q --force origin main --tags; }
# a version in Cargo.toml and in the crate's own line of Cargo.lock
set_crate_ver() { # dir name version
  [[ -f $1/Cargo.toml ]] || return 0
  sed -i '0,/^version = "/s/^version = ".*/version = "'"$3"'"/' "$1/Cargo.toml"
  [[ -f $1/Cargo.lock ]] &&   sed -i '/^name = "'"$2"'"$/{n;s/^version = .*/version = "'"$3"'"/}' "$1/Cargo.lock"
}
new_repo() { # name from-tree
  rm -rf "${SRC:?}/$1"
  git init -q -b main "$SRC/$1"
  snap_tree "$2" "$SRC/$1"
  git init -q --bare -b main "$GH/$1.git"
  git -C "$GH/$1.git" config uploadpack.allowFilter true
  git -C "$SRC/$1" remote add origin "$GH/$1.git"
}
# the shell at a version, with the modules.lock of that version
shell_tag() { # version
  local d="$SRC/yami" r
  printf '%s\n' "$1" >"$d/VERSION"
  set_crate_ver "$d/shell" yami-shell "$1"
  commit_tag "$d" "v$1"
  push yami
}
module_tag() { # module version
  set_crate_ver "$SRC/$1" "$1" "$2"
  commit_tag "$SRC/$1" "v$2"
  push "$1"
}
lock_all() { # version
  : >"$SRC/yami/modules.lock"
  local m
  for m in "${MODULES[@]}"; do printf '%s v%s\n' "$m" "$1" >>"$SRC/yami/modules.lock"; done
}
world() {
  say "building the fake GitHub"
  rm -rf "$WORK/github" "$SRC"
  mkdir -p "$GH" "$SRC" "$LOGS"
  [[ -f $WORK/key ]] || ssh-keygen -q -t ed25519 -N "" -f "$WORK/key" -C test
  [[ -f $WORK/rogue ]] || ssh-keygen -q -t ed25519 -N "" -f "$WORK/rogue" -C rogue
  make_shims
  # yami-px comes through cargo with its real history: Cargo.lock names its commit
  git clone -q --bare "$MODS/yami-px" "$GH/yami-px.git"
  new_repo yami "$ROOT"
  printf 'test namespaces="git" %s\n' "$(cut -d' ' -f1,2 "$WORK/key.pub")" >"$SRC/yami/keys/allowed_signers"
  rm -f "$SRC/yami/CLAUDE.md" "$SRC/yami/AGENTS.md" "$SRC/yami/GEMINI.md"
  local m
  for m in "${MODULES[@]}"; do
    new_repo "$m" "$MODS/$m"
    [[ $m == yami-niri ]] && stub_niri
    module_tag "$m" 3.0.0
  done
  lock_all 3.0.0
  shell_tag 3.0.0
  ok "repositories at 3.0.0"
}
# yami-niri without its 5–25 minute build: the same commands, a stand-in binary
stub_niri() {
  rm -rf "${SRC:?}/yami-niri/patches"
  cat >"$SRC/yami-niri/yami-niri.sh" <<'SH'
#!/usr/bin/env bash
VERSION="26.04-yami.test"
case ${1:-} in
  build)
    mkdir -p "$HOME/.local/lib/yami-niri/current"
    printf '#!/bin/sh\necho "niri 26.04-yami.test"\n' >"$HOME/.local/lib/yami-niri/current/niri"
    chmod +x "$HOME/.local/lib/yami-niri/current/niri"
    cp "$0" "$HOME/.local/lib/yami-niri/yami-niri.sh" ;;
  off | on | status) ;;
esac
SH
  chmod +x "$SRC/yami-niri/yami-niri.sh"
}

# ── a throwaway machine ─────────────────────────────────────────────────────
machine() { # fresh HOME; the build cache is shared between scenarios
  rm -rf "${H:?}"
  mkdir -p "$H/run" "$H/.cache/yami" "$WORK/target"
  chmod 700 "$H/run"
  ln -s "$WORK/target" "$H/.cache/yami/target"
}
sandbox() { # command…: everything the installer and the updater run goes through here
  env -i -C "$H" HOME="$H" USER=tester LOGNAME=tester LANG=en_US.UTF-8 TERM=dumb SHELL=/bin/bash \
    PATH="$SHIMS:$H/.local/bin:$REAL_HOME/.cargo/bin:/usr/bin:/bin" \
    XDG_RUNTIME_DIR="$H/run" \
    CARGO_HOME="$REAL_HOME/.cargo" RUSTUP_HOME="$REAL_HOME/.rustup" CARGO_NET_OFFLINE=true CARGO_NET_GIT_FETCH_WITH_CLI=true \
    CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=256 CARGO_PROFILE_RELEASE_OPT_LEVEL=0 CARGO_PROFILE_RELEASE_STRIP=false CARGO_BUILD_JOBS=8 \
    GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.file://$GH/.insteadOf" GIT_CONFIG_VALUE_0="https://github.com/Trendorin/" \
    "${SANDBOX_ENV[@]}" "$@"
}
SANDBOX_ENV=()
log_for() { printf '%s/%s.log' "$LOGS" "$1"; }
# bootstrap as the README says: the script read from a pipe, not from the repository
first_install() { # extra flags…
  SANDBOX_ENV+=()
  sandbox bash -c 'cat "$1" | bash -s -- "${@:2}"' _ "$SRC/yami/install.sh" --lang en "$@" >>"$(log_for "$SCEN")" 2>&1
}
inst() { sandbox bash "$H/.local/share/yami/src/install.sh" --lang en "$@" >>"$(log_for "$SCEN")" 2>&1; }
yami() { sandbox "$H/.local/bin/yami-shell" "$@" >>"$(log_for "$SCEN")" 2>&1; }
apply() { inst --apply "$1" --restyle "${2:-none}"; }
state() { printf '%s/.local/state/yami/%s' "$H" "$1"; }
ver_of() { sandbox "$H/.local/bin/$1" --version 2>/dev/null | awk '{print $2}'; }
is_ver() { [[ "$(ver_of "$1")" == "$2" ]]; }
installed_version() { sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$(state install.json)"; }
components() { sed -n 's/.*"components": *"\([^"]*\)".*/\1/p' "$(state install.json)"; }
has_comp() { [[ " $(components) " == *" $1 "* ]]; }
count_hits() { grep -c 'curl .*yami-installs.txt' "$H/calls.log" 2>/dev/null || true; }
data() { printf '%s/.local/share/%s' "$H" "$1"; }
cfg() { printf '%s/.config/%s' "$H" "$1"; }
bin_sums() { (cd "$H/.local/bin" && sha256sum -- "${BINS[@]}" 2>/dev/null); }
niri_ok() { sandbox /usr/bin/niri validate -c "$(cfg niri/config.kdl)"; }
# settings: ~/.config/yami/shell.json, before 3.0 ~/.config/yami-shell.json; one's own niri lines: in
# config.kdl since 3.0 (yami only adds its include), in user.kdl before (yami wrote config.kdl whole)
settings_file() { if [[ -f $(cfg yami/shell.json) ]]; then cfg yami/shell.json; else cfg yami-shell.json; fi; }
user_mark() {
  sandbox "$H/.local/bin/yami-shell" config set barMargin 13
  if grep -q 'yami/niri/yami.kdl' "$(cfg niri/config.kdl)" 2>/dev/null; then printf '// my tweak\n' >>"$(cfg niri/config.kdl)"; else printf '// my tweak\n' >>"$(cfg niri/user.kdl)"; fi
}
user_kept() {
  # -q: a match in either file is enough, the other may be missing
  grep -q '"barMargin": *13' "$(settings_file)" && grep -qsF '// my tweak' "$(cfg niri/config.kdl)" "$(cfg niri/user.kdl)"
}
publish() { # version: every module and the shell move to it
  local m
  for m in "${MODULES[@]}"; do module_tag "$m" "$1"; done
  lock_all "$1"
  shell_tag "$1"
}
update() { sandbox "$H/.local/bin/yami-shell" update apply >>"$(log_for "$SCEN")" 2>&1; }
reset_world() { world >/dev/null; }

# ── scenarios ───────────────────────────────────────────────────────────────
t_base() {
  machine
  check "the first install finishes" first_install --yes
  check "the shell is in place" is_ver yami-shell 3.0.0
  # the first-run setup is part of the shell now (welcome.rs)
  nocheck "no separate setup window" exists "$H/.local/bin/yami-setup"
  check "yami command" test -L "$H/.local/bin/yami"
  check "no module was installed" missing "$H/.local/bin/yami-files"
  check "the record says core" has_comp core
  check "the first-run mark is set" exists "$(state first-run)"
  check "niri accepts the config" niri_ok
  check "the source is remembered" contains "$(state source)" "/.local/share/yami/src"
  check "a rollback point is not made on a first install" missing "$(state snapshots)"
  [[ "$(count_hits)" == 1 ]] && ok "the counter asked once" || no "the counter asked once" "$(count_hits) requests"
  check "the counter request names only the URL" grep -qxE 'curl -fsSL -A yami --connect-timeout 4 --max-time 8 -o /dev/null https://github.com/Trendorin/yami/releases/download/installs/yami-installs.txt' "$H/calls.log"
  check "no Python was started" sh -c "! grep -q python '$(log_for "$SCEN")'"
  # installing again: the same machine, repaired, nothing asked to count twice
  check "running the installer again is a repair" inst --yes
  check "the repair keeps the first-run mark as it was" exists "$(state first-run)"
  [[ "$(count_hits)" == 1 ]] && ok "the counter stays at one" || no "the counter stays at one" "$(count_hits) requests"
}
t_pieces() { # one module added at a time, one taken away
  machine
  first_install --yes
  : >"$(state first-run)"
  check "files alone" apply "files"
  check "  yami-files is there" exists "$H/.local/bin/yami-files"
  check "  its .desktop" exists "$(data applications/yami-files.desktop)"
  check "  the file dialog portal" exists "$(data xdg-desktop-portal/portals/yami.portal)"
  check "  file dialogs go to yami-files" contains "$(cfg xdg-desktop-portal/niri-portals.conf)" "FileChooser=yami;gtk;"
  nocheck "  no player" exists "$H/.local/bin/yami-play"
  check "files and view" apply "files view"
  check "  yami-view is there" exists "$H/.local/bin/yami-view"
  check "view alone: files goes" apply "view"
  nocheck "  yami-files is gone" exists "$H/.local/bin/yami-files"
  nocheck "  the portal is gone" exists "$(data xdg-desktop-portal/portals/yami.portal)"
  nocheck "  folders no longer open in it" contains "$(cfg mimeapps.list)" "yami-files.desktop"
  nocheck "  no portal config without our backends" exists "$(cfg xdg-desktop-portal/niri-portals.conf)"
  check "agents alone" apply "agents"
  check "  the screen-share backend is ours" contains "$(cfg xdg-desktop-portal/niri-portals.conf)" "ScreenCast=yami-agents;gnome;"
  check "  file dialogs fall back to GTK" contains "$(cfg xdg-desktop-portal/niri-portals.conf)" "FileChooser=gtk;"
  check "everything" apply "files view play shot agents"
  for b in yami-files yami-view yami-play yami-shot yami-agents; do check "  $b" exists "$H/.local/bin/$b"; done
  check "one package transaction per run" sh -c "grep -c 'root: .*pacman -S' '$H/root.log' | grep -qx 6"
  check "OCR comes with screenshots" sh -c "grep 'root: .*pacman -S' '$H/root.log' | tail -1 | grep -q tesseract"
  check "drives come with files" sh -c "grep 'root: .*pacman -S' '$H/root.log' | tail -1 | grep -q udisks2"
  check "the root shell got its password from askpass-less sudo here" contains "$H/root.log" "bash -c"
}
t_module() { # one module on its own through a whole update, then back, then forward again
  local m=$1 repo=yami-$1 version_bin
  case $m in login) repo=yami-greet ;; fork) repo=yami-niri ;; esac
  version_bin=$repo
  reset_world
  machine
  first_install --yes
  check "installed $m on its own" apply "$m"
  user_mark
  [[ $m == files || $m == view || $m == play || $m == shot || $m == agents ]] && check "  $version_bin 3.0.0" is_ver "$version_bin" 3.0.0
  publish 3.1.0
  check "the update to 3.1.0 goes through" update
  check "  the record says 3.1.0" test "$(installed_version)" = 3.1.0
  check "  the shell is 3.1.0" is_ver yami-shell 3.1.0
  check "  the choice is kept" has_comp "$m"
  [[ $m == files || $m == view || $m == play || $m == shot || $m == agents ]] && check "  $version_bin is 3.1.0" is_ver "$version_bin" 3.1.0
  for b in files view play shot agents; do [[ $b == "$m" ]] || nocheck "  yami-$b did not appear" exists "$H/.local/bin/yami-$b"; done
  check "  settings and user.kdl kept" user_kept
  check "  niri accepts the config" niri_ok
  check "  a rollback point exists" sh -c "ls -d '$(state snapshots)'/*/ | grep -q ."
  [[ $m == fork ]] && check "  yami-niri was built" exists "$H/.local/lib/yami-niri/current/niri"
  [[ $m == login ]] && check "  the greeter is built, offered, not installed without root" sh -c "grep -q '^login ' '$(state packages-needed)' || true"
  check "the rollback goes through" yami update rollback
  check "  the shell is 3.0.0 again" is_ver yami-shell 3.0.0
  check "  the record says 3.0.0" test "$(installed_version)" = 3.0.0
  [[ $m == files || $m == view || $m == play || $m == shot || $m == agents ]] && check "  $version_bin is 3.0.0 again" is_ver "$version_bin" 3.0.0
  check "  settings kept through the rollback" user_kept
  check "updating again after a rollback" update
  check "  the shell is 3.1.0 again" is_ver yami-shell 3.1.0
  [[ $m == files || $m == view || $m == play || $m == shot || $m == agents ]] && check "  $version_bin is 3.1.0 again" is_ver "$version_bin" 3.1.0
}
t_full() {
  reset_world
  machine
  check "the full install" first_install --yes --full
  check "  no first-run window" missing "$(state first-run)"
  for b in "${BINS[@]}"; do check "  $b" exists "$H/.local/bin/$b"; done
  check "  the terminal and the night light come with the base" sh -c "grep 'root: .*pacman -S' '$H/root.log' | grep -q 'kitty.*wlsunset'"
  check "  no Python among the packages" sh -c "! grep 'root: .*pacman -S' '$H/root.log' | grep -q python"
  user_mark
  publish 3.1.0
  check "the full update" update
  for b in yami-files yami-view yami-play yami-shot yami-agents; do check "  $b 3.1.0" is_ver "$b" 3.1.0; done
  check "  settings kept" user_kept
  # the next release changes only one module: the others are not built again
  local before after
  before="$(stat -c %Y "$H/.local/bin/yami-files")"
  module_tag yami-view 3.2.0
  sed -i 's/^yami-view .*/yami-view v3.2.0/' "$SRC/yami/modules.lock"
  shell_tag 3.2.0
  sleep 1
  check "a release that touches one module" update
  check "  yami-view moved" is_ver yami-view 3.2.0
  after="$(stat -c %Y "$H/.local/bin/yami-files")"
  [[ $before == "$after" ]] && ok "  yami-files was not rebuilt" || no "  yami-files was not rebuilt"
  check "  yami-files is still 3.1.0" is_ver yami-files 3.1.0
  # uninstall takes it all back
  check "uninstall" inst --uninstall --yes
  for b in "${BINS[@]}"; do nocheck "  $b is gone" exists "$H/.local/bin/$b"; done
  nocheck "  the record is gone" exists "$(state install.json)"
  nocheck "  mimeapps forgot the apps" contains "$(cfg mimeapps.list)" "yami-view.desktop"
  check "  settings stay" exists "$(settings_file)"
  check "  the counter mark stays" exists "$(state counted)"
}
t_failures() {
  local before
  reset_world
  machine
  first_install --yes --full
  publish 3.1.0
  check "updated to 3.1.0" update

  # one module does not compile: nothing changes at all
  before="$(bin_sums)"
  printf '\ncompile_error!("broken on purpose");\n' >>"$SRC/yami-play/src/main.rs"
  module_tag yami-play 3.2.0
  sed -i 's/^yami-play .*/yami-play v3.2.0/' "$SRC/yami/modules.lock"
  shell_tag 3.2.0
  nocheck "a module that does not compile fails the update" update
  [[ "$before" == "$(bin_sums)" ]] && ok "  every binary is the old one" || no "  every binary is the old one"
  check "  the record stays at 3.1.0" test "$(installed_version)" = 3.1.0

  # the next release fixes it: the update goes through
  git -C "$SRC/yami-play" checkout -q HEAD~1 -- src/main.rs
  module_tag yami-play 3.3.0
  sed -i 's/^yami-play .*/yami-play v3.3.0/' "$SRC/yami/modules.lock"
  shell_tag 3.3.0
  check "a fixed release goes through" update
  check "  yami-play 3.3.0" is_ver yami-play 3.3.0

  # a tag signed by someone else
  before="$(bin_sums)"
  set_crate_ver "$SRC/yami-view" yami-view 3.4.0
  g -C "$SRC/yami-view" add -A && g -C "$SRC/yami-view" commit -q -m v3.4.0
  g -c "user.signingkey=$WORK/rogue.pub" -C "$SRC/yami-view" tag -s -m rogue v3.4.0
  push yami-view
  sed -i 's/^yami-view .*/yami-view v3.4.0/' "$SRC/yami/modules.lock"
  shell_tag 3.4.0
  nocheck "a tag signed by a stranger stops the update" update
  [[ "$before" == "$(bin_sums)" ]] && ok "  every binary is the old one" || no "  every binary is the old one"
  check "  it says why" contains "$(log_for "$SCEN")" "signature"
  g -C "$SRC/yami-view" tag -d v3.4.0 >/dev/null
  git -C "$GH/yami-view.git" tag -d v3.4.0 >/dev/null

  # the module's repository is unreachable
  sed -i 's/^yami-view .*/yami-view v3.1.0/' "$SRC/yami/modules.lock"
  shell_tag 3.5.0
  mv "$GH/yami-view.git" "$GH/gone.git"
  rm -rf "$H/.local/share/yami/modules/yami-view"
  before="$(bin_sums)"
  nocheck "a missing repository stops the update" update
  [[ "$before" == "$(bin_sums)" ]] && ok "  every binary is the old one" || no "  every binary is the old one"
  mv "$GH/gone.git" "$GH/yami-view.git"

  # yami's new niri part is broken: the installer has already swapped the binaries and must put them back
  before="$(bin_sums)"
  local old_kdl
  old_kdl="$(cat "$(cfg niri/config.kdl)")"
  printf 'this is not kdl {{{\n' >>"$SRC/yami/config/niri/base.kdl"
  shell_tag 3.6.0
  nocheck "a config niri rejects fails the update" update
  [[ "$before" == "$(bin_sums)" ]] && ok "  the snapshot put every binary back" || no "  the snapshot put every binary back"
  [[ "$old_kdl" == "$(cat "$(cfg niri/config.kdl)")" ]] && ok "  the config is the old one" || no "  the config is the old one"
  check "  niri accepts it" niri_ok
  git -C "$SRC/yami" checkout -q HEAD~1 -- config/niri/base.kdl
  shell_tag 3.7.0
  check "a fixed release goes through" update
  check "  the shell is 3.7.0" is_ver yami-shell 3.7.0
}
t_repair() { # the one-liner run again over a machine with chosen modules
  reset_world
  machine
  first_install --yes
  apply "files shot"
  rm -f "$(state first-run)"
  user_mark
  check "the installer run again" first_install --yes
  check "  modules stay" exists "$H/.local/bin/yami-files"
  check "  the record keeps them" has_comp files
  check "  and yami-shot" has_comp shot
  nocheck "  no first-run window appears" exists "$(state first-run)"
  check "  settings kept" user_kept
  publish 3.1.0
  check "the next update" update
  check "  still there" is_ver yami-files 3.1.0
  check "  and yami-shot" is_ver yami-shot 3.1.0
  nocheck "  nothing else came" exists "$H/.local/bin/yami-view"
}
t_privacy() {
  reset_world
  machine
  SANDBOX_ENV=(DO_NOT_TRACK=1)
  first_install --yes
  SANDBOX_ENV=()
  [[ "$(count_hits)" == 0 ]] && ok "DO_NOT_TRACK: no request" || no "DO_NOT_TRACK: no request"
  nocheck "  no mark either" exists "$(state counted)"
  machine
  first_install --yes --no-count
  [[ "$(count_hits)" == 0 ]] && ok "--no-count: no request" || no "--no-count: no request"
  machine
  SANDBOX_ENV=(YAMI_NO_COUNT=1)
  first_install --yes
  SANDBOX_ENV=()
  [[ "$(count_hits)" == 0 ]] && ok "YAMI_NO_COUNT: no request" || no "YAMI_NO_COUNT: no request"
}
t_askpass() { # the first-run setup's way in: no terminal, the password through SUDO_ASKPASS
  reset_world
  machine
  first_install --yes
  : >"$H/root.log"
  SANDBOX_ENV=(SUDO_ASKPASS=/usr/bin/false)
  check "the first-run setup's command" apply "files login" "gtk qt"
  SANDBOX_ENV=()
  check "  the root shell took the askpass" contains "$H/root.log" "sudo -A bash -c"
  check "  packages went through it" contains "$H/root.log" "root: "
  check "  the greeter binary too, after the build" contains "$H/root.log" "sudo -A $H/.local/share/yami/modules/yami-greet/install-system.sh"
  check "  the login step came first" sh -c "grep 'install-system.sh --system' '$H/root.log' | head -1 | grep -q root:"
}
t_standalone() { # a module taken on its own, from its repository, without the shell
  reset_world
  local m
  for m in yami-view yami-play yami-shot yami-agents yami-files; do
    machine
    rm -f "$SRC/$m/target"
    ln -s "$WORK/target" "$SRC/$m/target"
    check "$m: builds on its own" sandbox bash -c 'cd "$1" && cargo build --release --locked' _ "$SRC/$m"
    nocheck "  without a terminal or --yes it installs nothing" sandbox "$SRC/$m/target/release/$m" install
    nocheck "  nothing was put in" exists "$H/.local/bin/$m"
    check "  install --yes" sandbox "$SRC/$m/target/release/$m" install --yes
    check "  it runs" is_ver "$m" 3.0.0
    check "  uninstall takes it away" sandbox "$SRC/$m/target/release/$m" uninstall --yes
    nocheck "  the binary is gone" exists "$H/.local/bin/$m"
    rm -f "$SRC/$m/target"
  done
}
t_tty() { # the terminal side: a real pty, nobody at the keyboard but the Enter below
  reset_world
  machine
  local out="$LOGS/tty.typescript"
  # Enter after the first screen is up: a key pressed earlier is dropped when the menu starts reading
  check "the terminal install finishes in a pty" sandbox timeout -k 5 900 bash -c "(sleep 4; printf '\n') | TERM=xterm-256color script -qec 'bash \"$SRC/yami/install.sh\" --lang en' '$out'"
  check "  it drew the bar and the percent" grep -q '█.*%' "$out"
  check "  and the first screen" grep -q 'I will install only the base' "$out"
  check "  the cursor came back" grep -q $'\e\[?25h' "$out"
  check "  the base is there" exists "$H/.local/bin/yami-shell"
}
t_migrate() { # the real previous release (2.7.0, one repository) updates to this one through its own updater
  reset_world
  machine
  local d="$SRC/yami-old"
  rm -rf "$d"; mkdir -p "$d"
  git -C "$ROOT" archive v2.7.0 | tar -xC "$d"
  # the old release signed by the test key, on the same history as the new one
  printf 'test namespaces="git" %s\n' "$(cut -d' ' -f1,2 "$WORK/key.pub")" >"$d/keys/allowed_signers"
  rm -rf "$GH/yami.git"; git init -q --bare -b main "$GH/yami.git"; git -C "$GH/yami.git" config uploadpack.allowFilter true
  git init -q -b main "$d"; git -C "$d" remote add origin "$GH/yami.git"
  commit_tag "$d" v2.7.0
  g -C "$d" push -q origin main --tags
  sandbox bash -c 'cat "$1" | bash -s -- "${@:2}"' _ "$d/install.sh" --lang en --yes --no-sudo --no-monocraft --no-fork --no-login >>"$(log_for "$SCEN")" 2>&1
  check "2.7.0 is installed" test "$(installed_version)" = 2.7.0
  check "  with its own apps" exists "$H/.local/bin/yami-files"
  user_mark
  printf '%s\n' "ScreenCast=yami;" >/dev/null
  # the new tree on top of the old history, as a release
  (cd "$SRC/yami" && git ls-files -z --cached --others --exclude-standard | tar --null -T - -cf -) | tar -xC "$d"
  git -C "$d" ls-files -z | while IFS= read -r -d '' f; do [[ -e $SRC/yami/$f ]] || rm -f "$d/$f"; done
  printf '3.1.0\n' >"$d/VERSION"
  set_crate_ver "$d/shell" yami-shell 3.1.0
  local m
  for m in "${MODULES[@]}"; do module_tag "$m" 3.1.0; done
  : >"$d/modules.lock"
  for m in "${MODULES[@]}"; do printf '%s v3.1.0\n' "$m" >>"$d/modules.lock"; done
  commit_tag "$d" v3.1.0
  g -C "$d" push -q origin main --tags
  check "the old updater takes the new release" update
  check "  the record says 3.1.0" test "$(installed_version)" = 3.1.0
  check "  the shell is 3.1.0" is_ver yami-shell 3.1.0
  check "  its modules came over" is_ver yami-files 3.1.0
  check "  the new modules came in" is_ver yami-shot 3.1.0
  check "  and the services" is_ver yami-agents 3.1.0
  check "  settings and user.kdl kept" user_kept
  check "  niri accepts the config" niri_ok
  check "  screen sharing moved to yami-agents" contains "$(cfg xdg-desktop-portal/niri-portals.conf)" "ScreenCast=yami-agents"
  nocheck "  the embedded theme script is gone" exists "$(state theme.py)"
  check "  a retired group is not recorded" sh -c "! grep -q 'drives' '$(state install.json)'"
  # it says what is missing in this bare sandbox (no session): exit 1 is a report, anything above a crash
  check "  yami doctor runs" sh -c "'$H/.local/bin/yami-shell' doctor >/dev/null 2>&1; [ \$? -le 1 ]"
}

SCENARIOS=(base pieces standalone module-files module-view module-play module-shot module-agents module-login module-games module-fork full failures repair privacy askpass tty migrate)
run_scenario() {
  SCEN=$1
  : >"$(log_for "$SCEN")"
  say "$SCEN"
  case $1 in
    base) t_base ;;
    pieces) t_pieces ;;
    standalone) t_standalone ;;
    module-*) t_module "${1#module-}" ;;
    full) t_full ;;
    failures) t_failures ;;
    repair) t_repair ;;
    privacy) t_privacy ;;
    askpass) t_askpass ;;
    tty) t_tty ;;
    migrate) t_migrate ;;
    *) echo "unknown scenario: $1 (--list)" >&2; exit 2 ;;
  esac
}

[[ ${1:-} == --list ]] && { printf '%s\n' "${SCENARIOS[@]}"; exit 0; }
list=("$@")
((${#list[@]})) || list=("${SCENARIOS[@]}")
mkdir -p "$WORK" "$LOGS"
world
for s in "${list[@]}"; do run_scenario "$s"; done
printf '\n%s checks passed, %s failed\n' "$PASS" "$FAIL"
((FAIL)) && { printf '  %s\n' "${FAILED_NAMES[@]}"; exit 1; }
exit 0
