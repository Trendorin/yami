#!/usr/bin/env bash
# YAMI: the minimal install. One command, from anywhere, as yourself (not root):
#
#   bash <(curl -fsSL https://raw.githubusercontent.com/Trendorin/yami/main/install.sh)
#
# It puts in niri, the yami shell and what the desktop cannot start without. Everything else (files,
# player, screenshots, login screen, themes) is chosen in the graphical setup at the first login.
#
#   ./install.sh --full           the old way: everything now, no setup window
#   ./install.sh --yes            no questions
#   ./install.sh --dry-run        show the plan, change nothing
#   ./install.sh --uninstall      remove yami, restore what it replaced
#   ./install.sh --update         rebuild from this tree, keep every setting, no sudo (`yami update`)
#   ./install.sh --apply "files view …"   install exactly these modules (what the setup window runs)
#
# Choices: --lang en|ru|pt-BR  --palette blood|ash|sakura|amber|venom|ice|dusk  --theme dark|light
#   --look matte|glass|light|solid  --font sans|pixel  --fetch mini|full
#   --restyle LIST (all, none, or: gtk qt cursor kitty fastfetch btop firefox mangohud foot cava starship)
# Modules (with --full or --apply): files view play shot agents login games fork
#   --no-ID leaves one out, --ID takes it. --no-sudo: the home folder only, no packages.
# Rarely needed: --no-monocraft, --no-count, --keep-build, --no-anim, --print-packages.
# Run again over an installed yami: it updates and repairs, your choices and settings stay.
#
# Families: Arch, Fedora (incl. atomic), Debian/Ubuntu, openSUSE, Void, Alpine, Gentoo. NixOS: the
# flake (docs/PACKAGING.md). Anything replaced is kept next to itself as *.bak-DATE.
# Log: ~/.cache/yami/install.log. Each module in installer/*.sh holds one job.
set -uo pipefail
# the whole script is one block and the modules are read whole when sourced: bash parses all of it
# before running, so an update of these files mid-run (git pull, an editor) cannot shift lines
{

# ── bootstrap: started outside the repo (curl | bash) — fetch it and run from there ──
YAMI_REPO="${YAMI_REPO:-https://github.com/Trendorin/yami.git}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" 2>/dev/null && pwd)"
if [[ ! -f $ROOT/shell/Cargo.toml || ! -f $ROOT/installer/ui.sh ]]; then
  SRC_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/yami/src"
  if ! command -v git >/dev/null; then
    # a bare system: git is the one thing needed before the installer proper
    pm="" su=""
    for c in "pacman -S --needed --noconfirm git" "dnf install -y git" "apt-get install -y git" "zypper --non-interactive install git" "xbps-install -y git" "apk add git"; do
      command -v "${c%% *}" >/dev/null && { pm=$c; break; }
    done
    if ((EUID != 0)); then
      if command -v sudo >/dev/null; then su=sudo; elif command -v doas >/dev/null; then su=doas; fi
    fi
    if [[ -n $pm ]] && { ((EUID == 0)) || [[ -n $su ]]; } && { : </dev/tty; } 2>/dev/null; then
      echo "yami: git is needed to fetch the installer, installing it (the administrator password is asked once more later)" >&2
      # shellcheck disable=SC2086 # the command and its arguments
      $su $pm </dev/tty || { echo "yami: could not install git; install it yourself and run this again: $su $pm" >&2; exit 1; }
    else
      echo "yami: git is needed to fetch the installer; install it and run this again${pm:+: $su $pm}" >&2
      exit 1
    fi
  fi
  if [[ -d $SRC_DIR/.git ]]; then
    # a clone of an older address follows the current one; it may stand on a release tag
    [[ "$(git -C "$SRC_DIR" remote get-url origin 2>/dev/null)" == "$YAMI_REPO" ]] || git -C "$SRC_DIR" remote set-url origin "$YAMI_REPO"
    if git -C "$SRC_DIR" fetch --quiet origin main; then
      git -C "$SRC_DIR" checkout --quiet --detach FETCH_HEAD || echo "yami: could not update $SRC_DIR, using it as is" >&2
    else
      echo "yami: could not update $SRC_DIR, using it as is" >&2
    fi
  else
    echo "yami: downloading the installer…" >&2
    mkdir -p "${SRC_DIR%/*}"
    # history without file contents: small like --depth 1, but updates can compare versions
    git clone --filter=blob:none "$YAMI_REPO" "$SRC_DIR" || { echo "yami: git clone $YAMI_REPO failed" >&2; exit 1; }
  fi
  # curl … | bash: give the interactive installer the terminal back
  if [[ ! -t 0 ]] && { : </dev/tty; } 2>/dev/null; then exec bash "$SRC_DIR/install.sh" "$@" </dev/tty; fi
  exec bash "$SRC_DIR/install.sh" "$@"
fi

# ── where things go ─────────────────────────────────────────────────────────
CFG="${XDG_CONFIG_HOME:-$HOME/.config}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/yami"
STATE="${XDG_STATE_HOME:-$HOME/.local/state}/yami"
BIN="$HOME/.local/bin"
STAMP="$(date +%Y%m%d-%H%M%S)"
# settings of this shell: ~/.config/yami/shell.json; before 3.0 ~/.config/yami-shell.json (the shell moves it)
SET_JSON="$CFG/yami/shell.json"
[[ -f $SET_JSON || ! -f $CFG/yami-shell.json ]] || SET_JSON="$CFG/yami-shell.json"
LOG="$CACHE/install.log"
MISSING_F="$CACHE/missing-packages"
RUST_MIN="1.89"
NIRI_TAG="v26.04"
# ext-background-effect (blur only under the shell panels, not their shadow margins) and the
# config syntax need niri 26.04 or newer
NIRI_MIN="26.04"
# pinned and checked: a changed upload is refused (kitty then uses Terminess from the repo)
MONOCRAFT_URL="https://github.com/IdreesInc/Monocraft/releases/download/v4.2.1/Monocraft-nerd-fonts-patched.ttc"
MONOCRAFT_SHA256="67f88ff9e7c6560f6cf60fb062fd353a72f62dc2654462950c65b63ed53d9754"
[[ -d $HOME/.cargo/bin ]] && export PATH="$HOME/.cargo/bin:$PATH"

# ── flags ───────────────────────────────────────────────────────────────────
YES=0 SUDO=1 SHELL_ONLY=0 FULL=0 APPLY=0 REPAIR=0 APPLY_LIST="" RESTYLE="" COMP_FLAGS=() PALETTE="" LOOK_OPT="" THEME_OPT="" LANG_OPT="" FONT_OPT="" FETCH_OPT="" ANIM=1 DRY=0 UNINSTALL=0 UPDATE=0 COUNT=1 KEEP_BUILD=0 MONO="" PRINT_PKGS=0
while (($#)); do
  case "$1" in
    -y | --yes) YES=1 ;;
    --no-sudo) SUDO=0 ;;
    --shell-only) SHELL_ONLY=1 ;;
    --full) FULL=1 ;;
    --apply) APPLY=1 APPLY_LIST="${2:-}" YES=1 ANIM=0; shift ;;
    --restyle) RESTYLE="${2:-}"; shift ;;
    --no-restyle) RESTYLE=none ;;
    --font) FONT_OPT="${2:-}"; shift ;;
    --fetch) FETCH_OPT="${2:-}"; shift ;;
    --palette) PALETTE="${2:-}"; shift ;;
    --look) LOOK_OPT="${2:-}"; shift ;;
    --theme) THEME_OPT="${2:-}"; shift ;;
    --lang) LANG_OPT="${2:-}"; shift ;;
    --no-anim) ANIM=0 ;;
    --dry-run) DRY=1 ;;
    --print-packages) PRINT_PKGS=1 ;;
    --no-count) COUNT=0 ;;
    --keep-build) KEEP_BUILD=1 ;;
    --no-monocraft) MONO=0 ;;
    --monocraft) MONO=1 ;;
    --uninstall) UNINSTALL=1 ;;
    --update) UPDATE=1 YES=1 ANIM=0 SUDO=0 ;;
    -h | --help) sed -n '2,/^set -uo/p' "${BASH_SOURCE[0]:-$0}" | sed -e '$d' -e 's/^# \{0,1\}//'; exit 0 ;;
    # a component: --no-files, --games… (components.sh knows the names and refuses others)
    --no-[a-z]*) COMP_FLAGS+=("${1#--no-}=0") ;;
    --[a-z]*) COMP_FLAGS+=("${1#--}=1") ;;
    *) echo "unknown option: $1 (see --help)" >&2; exit 2 ;;
  esac
  shift
done
[[ -t 0 && -t 1 ]] || { YES=1; ANIM=0; }
# the first install is the base alone; the rest comes from the setup window. Run again over an
# installed yami it is a repair: the same choice, nothing is dropped and no first-run window
if ((!UPDATE && !FULL && !APPLY && !UNINSTALL)) && [[ -f $STATE/install.json && -x $BIN/yami-shell ]]; then
  REPAIR=1
fi
((UPDATE || FULL || APPLY || REPAIR)) || SHELL_ONLY=1
# git never asks for a login on the terminal: a moved or private repo fails instead of hanging
export GIT_TERMINAL_PROMPT=0

# ── the modules: each only defines (installer/*.sh) ─────────────────────────
for m in ui json detect root packages components restyle choices modules build apps desktop niri login backup uninstall summary; do
  # shellcheck source=/dev/null
  . "$ROOT/installer/$m.sh" || { echo "yami: $ROOT/installer/$m.sh is missing" >&2; exit 1; }
done
unset m

detect_distro
if ((PRINT_PKGS)); then
  packages_for_family
  print_packages
  exit 0
fi
# `sudo ./install.sh` would put the desktop into root's home: the installer asks for the password
# itself, only where it is needed (containers and CI run as root on purpose: YAMI_AS_ROOT=1)
if ((EUID == 0)) && [[ -z ${YAMI_AS_ROOT:-} && -z ${CI:-} ]]; then
  echo "yami: run the installer as yourself, not as root: it asks for the password when it needs it" >&2
  echo "      ./install.sh   (a root shell on purpose, e.g. a container: YAMI_AS_ROOT=1)" >&2
  exit 1
fi
mkdir -p "$CACHE"
: >"$LOG"
: >"$MISSING_F"

# ── language: the flag, then what the shell is set to (update), then the system ──
L=""
if [[ -n $LANG_OPT ]]; then
  L="$(lang_code "$LANG_OPT")"
  [[ -n $L ]] || { echo "--lang: en, ru or pt-BR" >&2; exit 2; }
elif ((UPDATE)) && [[ -f $SET_JSON ]]; then
  L="$(lang_code "$(json_field "$SET_JSON" lang 2>/dev/null)")"
fi
if [[ -z $L ]]; then
  L="$(lang_code "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}")"
  L="${L:-en}"
  LANG_ASK=1
else
  LANG_ASK=0
fi
# ${#s} must count characters, not bytes
case "$(locale charmap 2>/dev/null)" in UTF-8) ;; *) export LC_ALL=C.UTF-8 ;; esac

detect_machine
detect_login
packages_for_family
choices_init
paint
ui_traps
components_init
restyle_init
components_limit

# ── go: each module adds its own steps (plan_*), run_steps runs them in this order ──
((UNINSTALL)) && uninstall

if ((UPDATE)); then
  # rebuild from this tree with the choices of the first install; no packages, no sudo
  say "$(t "обновление из" "update from" "atualizar de") $ROOT"
  plan_snapshot # backup.sh: the rollback point
  plan_fetch    # modules.sh: each module's source, signature checked
  plan_build    # build.sh: the shell and the chosen apps
  plan_apps     # apps.sh: the modules' files (or out, when unticked)
  plan_desktop  # desktop.sh: fonts, niri config, everything from the palette
  plan_record   # desktop.sh: install record, packages-needed, counter, restart
  run_steps
  fork_after    # niri.sh: yami-niri when its patch series moved
  login_after   # login.sh: offers the new login screen
  summary_update
fi

# what the setup window asked for: only these modules, nothing of the base again
if ((APPLY)); then
  say "$(t "ставлю выбранное" "installing what you chose" "instalando o que você escolheu")"
  root_begin
  plan_snapshot
  plan_packages
  plan_fetch
  plan_login
  plan_build
  plan_apps
  plan_apply_look
  plan_record
  run_steps
  root_end
  login_after
  fork_after
  summary_update
fi

((YES)) || welcome
paint
[[ -t 1 ]] && printf '\e[H\e[J'
banner $((YES))
root_begin      # root.sh: the password once
plan_snapshot   # backup.sh: only over an installed yami

# root steps first: the root shell closes before the build (run_steps, root.sh)
plan_packages   # packages.sh (git and ssh-keygen too: the next step needs them)
plan_fetch      # modules.sh: the source of each chosen module
plan_niri       # niri.sh: upstream niri when the packaged one is too old
plan_services   # desktop.sh
plan_login      # login.sh: greetd (the last root step)
plan_build
plan_apps
plan_desktop
plan_record
run_steps
root_end        # root.sh
login_after     # login.sh: yami-greet itself
fork_after      # niri.sh: yami-niri (minutes)
summary_install # summary.sh
exit 0
}
