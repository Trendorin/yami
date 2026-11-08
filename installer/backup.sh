# The rollback point: before an update, a change of modules or a repair, the binaries and the files
# the installer rewrites are copied to ~/.local/state/yami/snapshots/<date>. An update that fails after
# it began changing things puts everything back by itself; `yami update rollback` and settings →
# updates use the newest one. `paths` lists every file with its class: G (generated: a rollback puts it
# back) or U (yours: only the automatic restore right after a failed run touches it). A file that did
# not exist at the snapshot is removed on restore. Three snapshots are kept.
# shellcheck shell=bash

SNAP_ROOT="$STATE/snapshots"
SNAP_KEEP=3
SNAP_NOW=""

# every file the installer may write outside its own cache, one per line
snapshot_list() {
  local data="${XDG_DATA_HOME:-$HOME/.local/share}" b n
  for b in yami-shell yami-setup yami-files yami-view yami-play yami-shot yami-agents; do printf 'G %s\n' "$BIN/$b"; done
  # yami's part of niri; before 3.0 it lay next to config.kdl
  for n in yami.kdl base.kdl binds.kdl rules.kdl colors.kdl cursor.kdl; do printf 'G %s\n' "$CFG/yami/niri/$n"; done
  for n in yami-local.kdl yami-binds.kdl yami-fx.kdl yami-colors.kdl yami-cursor.kdl; do printf 'G %s\n' "$CFG/niri/$n"; done
  printf 'G %s\n' "$CFG/xdg-desktop-portal/niri-portals.conf" "$CFG/environment.d/60-yami-path.conf" "$CFG/systemd/user/niri.service.d/60-yami-path.conf" "$STATE/install.json" \
    "$data/applications/yami-files.desktop" "$data/applications/yami-view.desktop" "$data/applications/yami-play.desktop" \
    "$data/xdg-desktop-portal/portals/yami.portal" "$data/xdg-desktop-portal/portals/yami-agents.portal" \
    "$data/dbus-1/services/org.freedesktop.impl.portal.desktop.yami.service" \
    "$data/dbus-1/services/org.freedesktop.impl.portal.desktop.yamiagents.service" \
    "$data/dbus-1/services/org.freedesktop.FileManager1.service"
  printf 'U %s\n' "$CFG/yami/shell.json" "$CFG/yami-shell.json" "$CFG/mimeapps.list" "$CFG/yami/niri/outputs.kdl" "$CFG/niri/yami-outputs.kdl" "$CFG/niri/user.kdl"
  # niri's config.kdl is the person's (only the restore right after a failed run touches it), except the
  # one yami before 3.0 wrote whole: a rollback to that version needs it back
  if grep -q '"yami-binds.kdl"' "$CFG/niri/config.kdl" 2>/dev/null; then n=G; else n=U; fi
  printf '%s %s\n' "$n" "$CFG/niri/config.kdl"
}

# on btrfs the copy costs nothing; busybox cp has no --reflink
snap_copy() { cp -a --reflink=auto -- "$1" "$2" 2>/dev/null || cp -a -- "$1" "$2"; }
do_snapshot() {
  local d="$SNAP_ROOT/$STAMP" cls p
  mkdir -p "$d/files" || return 1
  snapshot_list >"$d/paths"
  while read -r cls p; do
    [[ -e $p ]] || continue
    mkdir -p "$d/files${p%/*}" && snap_copy "$p" "$d/files$p" || return 1
  done <"$d/paths"
  # what was installed, for the list of rollback points
  printf '%s\n' "$(json_field "$STATE/install.json" version 2>/dev/null)" >"$d/version"
  printf '%s\n' "$d" >"$CACHE/snapshot-now"
  # the oldest go
  local old
  while IFS= read -r old; do rm -rf -- "$old"; done < <(ls -1d "$SNAP_ROOT"/*/ 2>/dev/null | sed 's:/$::' | sort | awk -v k="$SNAP_KEEP" '{ a[NR] = $0 } END { for (i = 1; i <= NR - k; i++) print a[i] }')
  return 0
}

# the run's own snapshot, right after a failure: everything back, your settings too
snapshot_restore() {
  local d cls p
  d="$(cat "$CACHE/snapshot-now" 2>/dev/null)"
  [[ -n $d && -s $d/paths ]] || return 1
  while read -r cls p; do
    if [[ -e $d/files$p || -L $d/files$p ]]; then
      mkdir -p "${p%/*}" && snap_copy "$d/files$p" "$p.yami-new" && mv -f -- "$p.yami-new" "$p" || return 1
    else
      rm -f -- "$p"
    fi
  done <"$d/paths"
  # the binaries are the old ones again: the next run must not think this source is installed
  rm -rf -- "$STATE/built"
  rm -f -- "$CACHE/snapshot-now"
  rm -rf -- "$d"
}

# only over an installed yami: there is something to go back to
plan_snapshot() { [[ -f $INSTALLED && -x $BIN/yami-shell ]] && add "$(t "точка отката" "rollback point" "ponto de restauração")" do_snapshot; return 0; }
