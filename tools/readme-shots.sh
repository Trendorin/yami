#!/usr/bin/env bash
# Real screenshots of a running yami session for the README (.github/assets): the plain desktop
# (clean.png), the desktop with the launcher, settings tabs, yami-files on a demo folder (闇 as in fastfetch on each palette's
# wallpaper), yami-view on one of them, the screenshot overlay (`yami-shot shot --frame`: the same drawing code
# with a selection and marks, over a real frame of the screen), the login screen (`yami-greet shot
# --size`: its own drawing code at the screen's size, with a made-up user and host, on the look the
# shell published); in English and Russian. yami-greet comes from this tree's build. Runs on an empty workspace with do-not-disturb on (no live notification or
# window gets into a picture), the generated pixel wallpaper instead of your own picture, sound
# devices under plain names (audioNames), drives hidden in yami-files, the network widget shown as
# a temperature graph (it names the interface: a VPN's, a Wi-Fi card's), no old update log in
# the updates tab, a plain bar (no tray, night light or Bluetooth: they show your programs and
# devices) and the apps listed in ~/.config/yami/shots-hide (desktop ids, one per line; a local
# file, never in the repository) hidden from the launcher; it puts the workspace, language,
# do-not-disturb, wallpaper, names, widgets, bar, the log and those apps back after.
# Refuses while a game or a fullscreen window has focus. Needs niri, grim.
# Usage: tools/readme-shots.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/.github/assets"
TMP="$(mktemp -d)"
OUT="$TMP/out"
mkdir -p "$OUT"
RU=0
case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in ru*) RU=1 ;; esac
t() { if ((RU)); then printf '%s' "$1"; else printf '%s' "$2"; fi; }
say() { printf '\e[38;2;196;22;28m▪\e[0m %s\n' "$*"; }
die() { printf '\e[31m✕ %s\e[0m\n' "$*" >&2; exit 1; }

[[ -n ${NIRI_SOCKET:-} ]] || die "$(t "нужен сеанс niri" "needs a niri session")"
f="$(niri msg --json focused-window 2>/dev/null || true)"
if grep -qiE '"app_id":"(steam_app_[0-9]+|gamescope|[^"]*\.exe)"' <<<"$f" || grep -q '"is_fullscreen":true' <<<"$f"; then
  die "$(t "в фокусе игра или окно на весь экран — не снимаю" "a game or a fullscreen window has focus: not shooting")"
fi
# README целиком в «пепле» (шапка рисуется yami-dev readme-art '#a8a29c'): в другой палитре не снимаем
acc="$(yami config get accent | tr -d '"')"
pal="$(yami config get palette | tr -d '"')"
if [[ $pal != ash || ( -n $acc && ${acc,,} != "#a8a29c" ) ]]; then
  die "$(t "README снимается в палитре «пепел» (сейчас: $pal ${acc:-}) — переключи её в настройках" "README shots use the ash palette (now: $pal ${acc:-}): switch to it in settings")"
fi
DEV="$ROOT/shell/target/release/yami-dev"
[[ -x $DEV ]] || cargo build --release --locked -p yami-dev --manifest-path "$ROOT/shell/Cargo.toml" || exit 1
dev() { "$DEV" shots "$@"; }
out="$(niri msg --json focused-output | dev out-name)"
size="$(niri msg --json focused-output | dev out-size)"
GREET="$ROOT/../yami-modules/yami-greet/target/release/yami-greet"
[[ -x $GREET ]] || die "$(t "нет $GREET — собери модуль yami-greet" "no $GREET: build the yami-greet module")"
back="$(niri msg --json workspaces | dev ws-focused)"
empty="$(niri msg --json workspaces | dev ws-empty "$out")"
lang0="$(yami config get lang)"
dnd0="$(yami config get dnd)"
names0="$(yami config get audioNames)"
wall0="$(yami config get wallpaper)"
widgets0="$(yami config get widgets)"
bar0=("$(yami config get barLeft)" "$(yami config get barCenter)" "$(yami config get barRight)")
apps_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
hide_list="${XDG_CONFIG_HOME:-$HOME/.config}/yami/shots-hide"
hidden=()
ulog="${XDG_CACHE_HOME:-$HOME/.cache}/yami/update.log"
pids=()
# демо-папка с коротким путём для строки адреса; удаляется, только если её создали мы
demo="${TMPDIR:-/tmp}/yami"
[[ -e $demo ]] && die "$(t "$demo уже есть — убери его" "$demo exists: remove it first")"
made=0
restore() {
  for p in "${pids[@]}"; do kill "$p" 2>/dev/null || true; done
  yami config set lang "$lang0" >/dev/null
  yami config set dnd "$dnd0" >/dev/null
  yami config set audioNames "$names0" >/dev/null
  yami config set wallpaper "$wall0" >/dev/null
  yami config set widgets "$widgets0" >/dev/null
  yami config set barLeft "${bar0[0]}" >/dev/null
  yami config set barCenter "${bar0[1]}" >/dev/null
  yami config set barRight "${bar0[2]}" >/dev/null
  for a in "${hidden[@]}"; do rm -f -- "$apps_dir/$a.desktop"; done
  [[ -f $TMP/update.log ]] && mv -f "$TMP/update.log" "$ulog"
  niri msg action focus-workspace "$back" >/dev/null 2>&1 || true
  rm -rf "$TMP"
  ((made)) && rm -rf "$demo"
  return 0
}
trap restore EXIT

# демо-папка: свои картинки проекта (обои всех семи палитр), ничего из домашней папки
made=1
mkdir -p "$demo"/{Documents,Downloads,Music,Projects,Pictures}
dev demo "$demo"

cfg="$TMP/cfg"
mkdir -p "$cfg"
mkdir -p "$cfg/yami"
for f in yami/shell.json yami/files.json yami/style.json user-dirs.dirs; do
  [[ -f ${XDG_CONFIG_HOME:-$HOME/.config}/$f ]] && cp "${XDG_CONFIG_HOME:-$HOME/.config}/$f" "$cfg/$f"
done
# диски и флешки — не в кадр
dev files-no-devices "$cfg/yami/files.json"
# звуковые устройства — простыми именами вместо моделей: микрофон и наушники
names() { # lang
  pactl -f json list sinks 2>/dev/null | dev audio-names "$1"
}

# перед каждым кадром: мы всё ещё на пустом столе и на нём только наши окна (yami-files,
# yami-view) — иначе в кадр попадёт чужое (ты переключил стол, открылось окно), бросаем всё
guard() {
  niri msg --json workspaces >"$TMP/ws.json" && niri msg --json windows >"$TMP/win.json" || die "niri"
  dev guard "$TMP/ws.json" "$TMP/win.json" "$empty" "$out" || die "$(t "на пустом столе что-то изменилось — съёмка прервана, картинки не тронуты" "the empty workspace changed: shooting stopped, pictures untouched")"
}
shoot() { sleep "${2:-1.2}"; guard; grim -o "$out" "$TMP/$1.png"; guard; }
# панель по центру экрана с полями (видно затемнённый стол вокруг)
crop() { # name w h
  dev crop "$TMP/$1.png" "$2" "$3" "$4"
}
full() { dev rgb "$TMP/$1.png" "$2"; }

say "$(t "пустой рабочий стол $empty, не беспокоить, пиксельные обои" "empty workspace $empty, do not disturb, the pixel wallpaper")"
yami config set dnd true >/dev/null
yami config set wallpaper '""' >/dev/null
# the network widget names the interface: a temperature graph in its place, same spot and size
yami config set widgets "$(dev widgets-no-net "$widgets0")" >/dev/null
# a plain bar: what the README is about, nothing of this machine
yami config set barLeft '["launcher", "sep", "workspaces"]' >/dev/null
yami config set barCenter '["tasks"]' >/dev/null
yami config set barRight '["kb", "sep", "cpu", "ram", "temp", "sep", "vol", "clock", "sep", "settings", "power"]' >/dev/null
# apps of yours kept out of the launcher: a user .desktop with Hidden=true overrides the system one
# (only where you have no file of your own under that name; removed again in restore)
if [[ -r $hide_list ]]; then
  mkdir -p "$apps_dir"
  while read -r a; do
    [[ $a =~ ^[A-Za-z0-9._-]+$ && ! -e $apps_dir/$a.desktop ]] || continue
    printf '[Desktop Entry]\nType=Application\nName=%s\nHidden=true\n' "$a" >"$apps_dir/$a.desktop"
    hidden+=("$a")
  done <"$hide_list"
fi
# the updates tab draws the tail of the last update's log: an older version there would contradict the header
[[ -f $ulog ]] && mv -f "$ulog" "$TMP/update.log"
niri msg action focus-workspace "$empty"
for lang in en ru; do
  say "$(t "язык" "language"): $lang"
  yami config set lang "$lang" >/dev/null
  yami config set audioNames "$(names "$lang")" >/dev/null
  sleep 2
  # чистый стол — один на оба README
  if [[ $lang == en ]]; then
    shoot "clean"
    full clean "$OUT/clean.png"
  fi
  # экран входа: его код рисования, размер экрана, вид из того, что выложил шелл
  "$GREET" shot --lang "$lang" --size "$size" -o "$TMP/greet" >/dev/null
  full greet/greet-pass "$OUT/login-$lang.png"
  yami msg launcher
  shoot "desktop"
  yami msg launcher
  full desktop "$OUT/desktop-$lang.png"
  for tab in color updates agents keys; do
    yami msg "settings $tab"
    shoot "settings-$tab"
    crop "settings-$tab" 780 572 "$OUT/settings-$tab-$lang.png"
  done
  yami msg settings
  yami msg "popup sound"
  shoot "sound" 1
  dev crop-top-right "$TMP/sound.png" 560 300 "$OUT/popup-sound-$lang.png"
  yami msg "popup sound"
  # свой XDG_CONFIG_HOME: вид и папки XDG те же, а закладок GTK нет
  XDG_CONFIG_HOME="$cfg" yami-files "$demo" >/dev/null 2>&1 &
  pids+=($!)
  sleep 1.5
  niri msg action maximize-column
  shoot "files" 2
  full files "$OUT/files-$lang.png"
  # оверлей скриншота: выделение и метки (код снимка шелла) поверх настоящего кадра с файлами
  yami-shot shot --lang "$lang" --frame "$TMP/files.png" -o "$TMP/snap" >/dev/null
  full snap/edit "$OUT/screenshot-$lang.png"
  kill "${pids[@]}" 2>/dev/null || true
  pids=()
  # просмотрщик — на обоях «сакуры»
  yami-view "$demo/sakura.png" >/dev/null 2>&1 &
  pids+=($!)
  shoot "view" 2
  full view "$OUT/view-$lang.png"
  kill "${pids[@]}" 2>/dev/null || true
  pids=()
  sleep 1
done
cp "$OUT"/*.png "$DEST/"
say "$(t "готово:" "done:") $DEST"
