#!/usr/bin/env bash
# Build yami-shell and swap it into the running session: ~/.local/bin/yami-shell, then restart.
# Refuses while a game or any fullscreen window has focus (the restart flashes the bar and
# panels); --force skips that guard.
#   --modules       also build and install the modules next to this repository (YAMI_MODULES_DIR)
#   --modules-only  only the modules, the shell keeps running
# The login screen needs root: yami-greet/install-system.sh.
# Usage: tools/deploy.sh [--force] [--modules | --modules-only]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="$HOME/.local/bin/yami-shell"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
FORCE=0 WITH_MODULES=0 ONLY_MODULES=0
for a in "$@"; do
  case $a in
    --force) FORCE=1 ;;
    --modules) WITH_MODULES=1 ;;
    --modules-only) WITH_MODULES=1 ONLY_MODULES=1 ;;
    *) echo "usage: tools/deploy.sh [--force] [--modules | --modules-only]" >&2; exit 2 ;;
  esac
done
# shellcheck source=/dev/null
[[ -f $MODS/env.sh ]] && . "$MODS/env.sh"

if ((!FORCE && !ONLY_MODULES)) && command -v niri >/dev/null && [[ -n ${NIRI_SOCKET:-} ]]; then
  focus="$(niri msg --json focused-window 2>/dev/null || true)"
  if grep -qiE '"app_id":"(steam_app_[0-9]+|gamescope|.*\.exe)"' <<<"$focus" ||
    grep -q '"is_fullscreen":true' <<<"$focus"; then
    echo "deploy: a game / fullscreen window has focus, not restarting the shell (use --force)" >&2
    exit 3
  fi
fi

# a copy next to the target, then a rename: a running program keeps its old file, a full disk
# leaves the old binary in place instead of a cut one
put() { install -Dm755 "$ROOT/shell/target/release/$1" "$HOME/.local/bin/.$1.new" && mv -f "$HOME/.local/bin/.$1.new" "$HOME/.local/bin/$1"; }
if ((WITH_MODULES)); then
  for m in yami-files yami-view yami-play yami-shot yami-agents; do
    [[ -x $MODS/$m/install.sh ]] || { echo "deploy: no $MODS/$m" >&2; exit 1; }
    "$MODS/$m/install.sh"
  done
  # running services move to the new binaries as soon as nothing is open (a stream is not cut)
  "$HOME/.local/bin/yami-files" --portal-reload || true
  "$HOME/.local/bin/yami-agents" --reload || true
fi
if ((ONLY_MODULES)); then
  echo "deploy: modules installed"
  exit 0
fi
cargo build --release --locked --workspace --manifest-path "$ROOT/shell/Cargo.toml"
put yami-setup
put yami-shell
ln -sfn yami-shell "$HOME/.local/bin/yami"
# what runs now (`yami update` compares against it); a dirty tree is marked as such
"$BIN" update stamp "$ROOT" || true
# alive = answers on its socket (not pgrep: `yami update apply` is a yami-shell process too)
# the new shell binds its socket anew: with an inode given, a ping counts only from another one
ino() { stat -c %i "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/yami-shell.sock" 2>/dev/null || echo none; }
up() {
  local i old=${1:-}
  for i in $(seq 1 80); do
    sleep 0.1
    [[ -n $old && $(ino) == "$old" ]] && continue
    "$BIN" msg ping >/dev/null 2>&1 && return 0
  done
  return 1
}
started=0
if "$BIN" msg ping >/dev/null 2>&1; then
  # exec of the new binary in place: same process, niri's autostart loop stays
  old="$(ino)"
  "$BIN" msg restart >/dev/null 2>&1 || true
  up "$old" && started=1
fi
if ((!started)) && [[ -n ${NIRI_SOCKET:-} ]]; then
  niri msg action spawn-sh -- "until $BIN; do sleep 1; done"
  up && started=1
fi
# the shell itself: yami-shell without arguments (CLI commands like `update apply` have them)
pid="$(pgrep -ax yami-shell | awk 'NF == 2 { print $1; exit }' || true)"
if ((started)) && [[ -n $pid ]]; then
  echo "deploy: yami-shell $("$BIN" --version | cut -d' ' -f2) running, pid $pid, $(($(awk '/VmRSS/{print $2}' "/proc/$pid/status") / 1024)) MB"
else
  echo "deploy: installed to $BIN (no niri session, not started)"
fi
