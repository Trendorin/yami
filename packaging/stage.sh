#!/usr/bin/env bash
# Put the ready binaries where a system-wide package wants them. For packagers: PKGBUILD, .deb, RPM,
# ebuild call it with the package's root in --destdir. A home-folder install is install.sh's job.
#   stage.sh --prefix /usr --destdir "$pkgdir" [--managed] [--only yami-shell|yami-files|…]
# --managed leaves the mark that hands updates and modules to the package manager (the updater stays off).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX=/usr DESTDIR="" MANAGED=0 ONLY=""
while (($#)); do
  case $1 in
    --prefix) PREFIX="${2:?}"; shift ;;
    --destdir) DESTDIR="${2:?}"; shift ;;
    --managed) MANAGED=1 ;;
    --only) ONLY="${2:?}"; shift ;;
    *) echo "usage: stage.sh --prefix DIR [--destdir DIR] [--managed] [--only PROGRAM]" >&2; exit 2 ;;
  esac
  shift
done
D="$DESTDIR$PREFIX"
want() { [[ -z $ONLY || $ONLY == "$1" ]]; }
service() { # the source file, the program it starts
  local to="$D/share/dbus-1/services/${1##*/}"
  install -d "${to%/*}"
  sed "s|@BIN@|$PREFIX/bin/$2|" "$1" >"$to"
  chmod 644 "$to"
}
licence() { install -Dm644 "$HERE/LICENSE" "$D/share/licenses/$1/LICENSE"; }
M="$HERE/modules"
if want yami-shell; then
  install -Dm755 "$HERE/bin/yami-shell" "$D/bin/yami-shell"
  ln -sfn yami-shell "$D/bin/yami"
  # the first login of a packaged install: niri's own config does not start the shell yet
  install -Dm644 "$HERE/config/autostart/yami-shell.desktop" "$DESTDIR/etc/xdg/autostart/yami-shell.desktop"
  install -Dm644 -t "$D/share/fonts/yami" "$HERE"/fonts/*.ttf
  install -Dm644 -t "$D/share/licenses/yami-shell" "$HERE"/fonts/OFL-*.txt
  install -Dm644 -t "$D/share/yami" "$HERE/config/kitty/yami.conf" "$HERE/config/fontconfig/50-yami-pixel.conf"
  ((MANAGED)) && install -Dm644 /dev/null "$D/share/yami/package-managed"
  licence yami-shell
fi
for m in yami-files yami-view yami-play yami-shot yami-agents; do
  want "$m" || continue
  install -Dm755 "$HERE/bin/$m" "$D/bin/$m"
  licence "$m"
  [[ -f $M/$m/config/$m.desktop ]] && install -Dm644 "$M/$m/config/$m.desktop" "$D/share/applications/$m.desktop"
done
if want yami-files; then
  install -Dm644 "$M/yami-files/config/yami.portal" "$D/share/xdg-desktop-portal/portals/yami.portal"
  service "$M/yami-files/config/org.freedesktop.impl.portal.desktop.yami.service" yami-files
  # no org.freedesktop.FileManager1 here: Nautilus owns that system file, and «show in folder» for every
  # app is each person's choice (`yami-files install` asks and puts it into the home folder)
fi
if want yami-agents; then
  install -Dm644 "$M/yami-agents/config/yami-agents.portal" "$D/share/xdg-desktop-portal/portals/yami-agents.portal"
  service "$M/yami-agents/config/org.freedesktop.impl.portal.desktop.yamiagents.service" yami-agents
fi
if want yami-greet; then
  # the login screen finds bin/yami-greet from libexec/yami and its fonts in share/yami-greet/fonts (docs/PACKAGING.md)
  install -Dm755 "$HERE/bin/yami-greet" "$D/bin/yami-greet"
  install -Dm755 "$M/yami-greet/greet-session" "$D/libexec/yami/greet-session"
  install -Dm644 -t "$D/share/yami-greet/fonts" "$M/yami-greet/fonts/Tiny5.ttf" "$M/yami-greet/fonts/DotGothic16-Regular.ttf"
  licence yami-greet
fi
