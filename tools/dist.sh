#!/usr/bin/env bash
# Ready binaries that run on every glibc distribution: the shell and the modules linked against an old
# glibc, one archive with everything the installer and the packages need, SHA256SUMS over it.
#   tools/dist.sh                  x86_64 archive into dist/ (the sysroot below when this system is newer)
#   tools/dist.sh --deb            also one .deb per program
#   tools/dist.sh --loose          also each module as one file: `yami-files install` from the binary alone
#   tools/dist.sh --sign KEY       also SHA256SUMS.sig, checked by the installer against keys/allowed_signers
#   tools/dist.sh --no-sysroot     link against this system's glibc (a container with an old one: CI)
#   tools/dist.sh --strict         stop when a module is not at the version of modules.lock (a release)
# A container with Debian 12 needs no sysroot; without containers the sysroot is the packages of Debian 12
# unpacked into ~/.cache/yami/sysroot and clang pointed at them. The binaries come out the same.
# Nothing is published here: tools/release.sh does that.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
# shellcheck source=/dev/null
[[ -f $MODS/env.sh ]] && . "$MODS/env.sh"
# the newest glibc the binaries may ask for. Linked against Debian 12 they ask for 2.34 at most: Ubuntu 22.04,
# Debian 12, Fedora 35, openSUSE Leap 16 and everything newer. installer/prebuilt.sh checks the same number
GLIBC_MAX=2.34
OUT="$ROOT/dist" KEY="" DEB=0 LOOSE=0 SYSROOT=1 STRICT=0
while (($#)); do
  case $1 in
    --deb) DEB=1 ;;
    --loose) LOOSE=1 ;;
    --sign) KEY="${2:?--sign KEY}"; shift ;;
    --no-sysroot) SYSROOT=0 ;;
    --strict) STRICT=1 ;;
    --out) OUT="${2:?--out DIR}"; shift ;;
    *) echo "usage: tools/dist.sh [--deb] [--loose] [--sign KEY] [--strict] [--no-sysroot] [--out DIR]" >&2; exit 2 ;;
  esac
  shift
done
VER="$(cat "$ROOT/VERSION")"
ARCH=x86_64
[[ "$(uname -m)" == "$ARCH" ]] || { echo "dist: only $ARCH for now" >&2; exit 1; }
NAME="yami-$VER-$ARCH-linux"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/yami"
TARGET="$CACHE/dist-target"
# the modules, each from its own repository; the shell and its services (yami-agents) come from this tree
PROGS=(yami-files yami-view yami-play yami-shot yami-greet)
say() { printf '\e[1m── %s\e[0m\n' "$*"; }
ver_gt() { [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$1" && $1 != "$2" ]]; }

# ── the sysroot: Debian 12's headers and libraries, each file checked against Debian's signed index ──
# What is linked from it ends up inside every released binary (crt1.o, libc_nonshared.a), so the chain is
# checked from the top: Debian's signature on the index (gpgv with Debian's archive keys, the package
# debian-archive-keyring on Arch, Fedora and Debian), the index's checksum of the package list, the list's
# checksum of each package. Without those keys the index rests on HTTPS alone: said out loud, and the
# sysroot remembers it (.done), so a release build repeats the warning until it is rebuilt checked.
DEBS=(libc6 libc6-dev linux-libc-dev libgcc-12-dev libgcc-s1 libgomp1 libitm1 libatomic1 libasan8 liblsan0 libtsan2 libubsan1 libquadmath0
  libcrypt1 libcrypt-dev libxkbcommon0 libxkbcommon-dev libpam0g libpam0g-dev libaudit1 libcap-ng0)
SYS="$CACHE/sysroot/debian-12-amd64"
debian_keyring() {
  local k
  for k in /usr/share/keyrings/debian-archive-keyring.gpg /usr/share/keyrings/debian-archive-keyring.pgp; do
    [[ -s $k ]] && { printf '%s\n' "$k"; return 0; }
  done
  return 1
}
unverified_note() {
  printf '  \e[33mwarning:\e[0m %s\n           %s\n' "the sysroot's package index is not checked against Debian's signature (HTTPS only):" \
    "install debian-archive-keyring (and gpgv), then remove $SYS to rebuild it checked" >&2
}
make_sysroot() {
  if [[ -f $SYS/.done ]]; then
    # a sysroot made before the signature was checked, or without Debian's keys
    if ((STRICT)) && [[ "$(cat "$SYS/.done")" != verified ]]; then unverified_note; fi
    return 0
  fi
  command -v clang >/dev/null && command -v ld.lld >/dev/null && command -v bsdtar >/dev/null || { echo "dist: the sysroot build needs clang, lld and bsdtar" >&2; return 1; }
  say "sysroot: Debian 12"
  local mirror=https://deb.debian.org/debian tmp idx p want f ring state=unverified
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' RETURN
  curl -fsSL --proto '=https' -o "$tmp/InRelease" "$mirror/dists/bookworm/InRelease"
  curl -fsSL --proto '=https' -o "$tmp/Packages.xz" "$mirror/dists/bookworm/main/binary-amd64/Packages.xz"
  # only the text Debian signed is read: gpgv writes it out, lines put around the signed block never count
  if ring="$(debian_keyring)" && command -v gpgv >/dev/null; then
    gpgv -q --keyring "$ring" --output "$tmp/Release" "$tmp/InRelease" 2>"$tmp/gpgv.log" ||
      { cat "$tmp/gpgv.log" >&2; echo "dist: the signature of Debian's package index does not check out" >&2; return 1; }
    state=verified
  else
    unverified_note
    cp "$tmp/InRelease" "$tmp/Release"
  fi
  want="$(awk '/^SHA256:/ { on = 1; next } /^[A-Z]/ { on = 0 } on && $3 == "main/binary-amd64/Packages.xz" { print $1 }' "$tmp/Release")"
  [[ -n $want && "$(sha256sum "$tmp/Packages.xz" | cut -d' ' -f1)" == "$want" ]] || { echo "dist: the package index does not match" >&2; return 1; }
  idx="$tmp/Packages"
  xz -dc "$tmp/Packages.xz" >"$idx"
  rm -rf -- "$SYS"
  mkdir -p "$SYS"
  for p in "${DEBS[@]}"; do
    read -r f want < <(awk -v p="$p" 'BEGIN { RS = ""; FS = "\n" } $1 == "Package: " p { for (i = 1; i <= NF; i++) { if ($i ~ /^Filename: /) f = substr($i, 11); if ($i ~ /^SHA256: /) s = substr($i, 9) } print f, s; exit }' "$idx")
    [[ -n $f && -n $want ]] || { echo "dist: $p is not in the index" >&2; return 1; }
    printf '  %s\n' "${f##*/}"
    curl -fsSL --proto '=https' -o "$tmp/p.deb" "$mirror/$f"
    [[ "$(sha256sum "$tmp/p.deb" | cut -d' ' -f1)" == "$want" ]] || { echo "dist: $p does not match the index" >&2; return 1; }
    bsdtar -xOf "$tmp/p.deb" 'data.tar.*' | bsdtar -xf - -C "$SYS"
  done
  # links with absolute targets (/lib64/ld-linux…) must stay inside the sysroot
  local l t
  while IFS= read -r -d '' l; do
    t="$(readlink "$l")"
    ln -sfn "$(realpath -m --relative-to="$(dirname "$l")" "$SYS$t")" "$l"
  done < <(find "$SYS" -type l -lname '/*' -print0)
  printf '%s\n' "$state" >"$SYS/.done"
}

# ── build ───────────────────────────────────────────────────────────────────
host_glibc="$(getconf GNU_LIBC_VERSION | awk '{print $2}')"
ENVS=()
if ((SYSROOT)) && ver_gt "$host_glibc" "$GLIBC_MAX"; then
  make_sysroot
  flags=(-C linker=clang -C "link-arg=--target=x86_64-linux-gnu" -C "link-arg=--sysroot=$SYS" -C link-arg=-fuse-ld=lld)
  ENVS=("CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=${flags[*]}" "CC=clang" "CFLAGS=--target=x86_64-linux-gnu --sysroot=$SYS"
    "PKG_CONFIG_SYSROOT_DIR=$SYS" "PKG_CONFIG_LIBDIR=$SYS/usr/lib/x86_64-linux-gnu/pkgconfig:$SYS/usr/share/pkgconfig" "PKG_CONFIG_ALLOW_CROSS=1")
fi
# --target: build scripts and proc-macros stay on the host's own libraries, only the result uses the sysroot
build() { # the crate's Cargo.toml, cargo arguments
  env ${ENVS[@]+"${ENVS[@]}"} cargo build --release --locked --target x86_64-unknown-linux-gnu --target-dir "$TARGET" --manifest-path "$@"
}
BUILT="$TARGET/x86_64-unknown-linux-gnu/release"
say "yami-shell $VER"
build "$ROOT/shell/Cargo.toml" -p yami-shell -p yami-agents
locked() { awk -v r="$1" '$1 == r { print $2 }' "$ROOT/modules.lock"; }
for m in "${PROGS[@]}"; do
  [[ -f $MODS/$m/Cargo.toml ]] || { echo "dist: no $MODS/$m" >&2; exit 1; }
  at="$(git -C "$MODS/$m" describe --tags --exact-match HEAD 2>/dev/null || echo "?")"
  if [[ $at != "$(locked "$m")" ]]; then
    printf '  warning: %s is at %s, modules.lock says %s\n' "$m" "$at" "$(locked "$m")"
    ((STRICT)) && exit 1
  fi
  say "$m"
  build "$MODS/$m/Cargo.toml"
done

# ── the tree inside the archive: what the installer and the packages take from it ─────────
say "archive"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
T="$STAGE/$NAME"
mkdir -p "$T/bin" "$T/modules"
strip_to() { install -m755 "$BUILT/$1" "$T/bin/$1"; }
strip_to yami-shell
strip_to yami-agents
for m in "${PROGS[@]}"; do strip_to "$m"; done
glibc_of() { objdump -T "$1" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -n1 | cut -d_ -f2; }
for b in "$T"/bin/*; do
  g="$(glibc_of "$b")"
  printf '  %-12s %6s KiB  glibc %s\n' "${b##*/}" "$(($(stat -c %s "$b") / 1024))" "$g"
  ver_gt "$g" "$GLIBC_MAX" && { echo "dist: ${b##*/} asks for glibc $g, newer than $GLIBC_MAX" >&2; exit 1; }
done
cp -a "$ROOT/config" "$ROOT/fonts" "$T/"
install -m644 "$ROOT/LICENSE" "$ROOT/VERSION" "$ROOT/modules.lock" "$T/"
install -m755 "$ROOT/packaging/stage.sh" "$T/stage.sh"
# what each module keeps next to its binary: .desktop files, portals, D-Bus services, the login screen's scripts
for m in yami-files yami-view yami-play; do
  mkdir -p "$T/modules/$m"
  cp -a "$MODS/$m/config" "$T/modules/$m/"
done
# the services' portal and D-Bus files, where stage.sh and the packages look for them
mkdir -p "$T/modules/yami-agents"
cp -a "$ROOT/shell/agents/config" "$T/modules/yami-agents/"
mkdir -p "$T/modules/yami-shot" "$T/modules/yami-greet"
cp -a "$MODS/yami-greet/fonts" "$MODS/yami-greet/greet-session" "$MODS/yami-greet/install-system.sh" "$T/modules/yami-greet/"
mkdir -p "$OUT"
ARCHIVE="$NAME.tar.gz"
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$(git -C "$ROOT" log -1 --format=%ct)" -C "$STAGE" -cf - "$NAME" | gzip -n9 >"$OUT/$ARCHIVE"

# ── .deb: built by hand (ar + tar), so any system can make them ─────────────
make_deb() { # package, stage.sh target
  local pkg=$1 deps=$2 recommends=${3:-} dir ctl
  dir="$(mktemp -d)" ctl="$(mktemp -d)"
  chmod 755 "$dir" # mktemp makes it 0700, and the tar's «./» would carry that
  "$T/stage.sh" --prefix /usr --destdir "$dir" --managed --only "$pkg" >/dev/null
  {
    printf 'Package: %s\nVersion: %s\nArchitecture: amd64\nMaintainer: Trendorin <193045225+Trendorin@users.noreply.github.com>\n' "$pkg" "$VER"
    printf 'Installed-Size: %s\nDepends: %s\n' "$(du -sk "$dir" | cut -f1)" "$deps"
    [[ -n $recommends ]] && printf 'Recommends: %s\n' "$recommends"
    printf 'Section: x11\nPriority: optional\nHomepage: https://github.com/Trendorin/yami\nDescription: %s, part of the yami desktop for niri\n' "$pkg"
  } >"$ctl/control"
  (cd "$dir" && find . -type f -printf '%P\0' | LC_ALL=C sort -z | xargs -0 md5sum) >"$ctl/md5sums"
  local tars=(--sort=name --owner=0 --group=0 --numeric-owner --mtime="@$(git -C "$ROOT" log -1 --format=%ct)")
  tar "${tars[@]}" -C "$ctl" -cJf "$ctl/control.tar.xz" control md5sums
  tar "${tars[@]}" -C "$dir" -cJf "$ctl/data.tar.xz" .
  printf '2.0\n' >"$ctl/debian-binary"
  local to="$OUT/${pkg}_${VER}_amd64.deb"
  rm -f -- "$to"
  (cd "$ctl" && ar rcD "$to" debian-binary control.tar.xz data.tar.xz)
  rm -rf -- "$dir" "$ctl"
}
if ((DEB)); then
  say ".deb"
  base="libc6 (>= $GLIBC_MAX), libgcc-s1, libxkbcommon0"
  make_deb yami-shell "$base, libpam0g, polkitd | policykit-1" "niri, yami-files, yami-view, yami-play, yami-shot, udisks2, gvfs-backends, gvfs-fuse, grim, wl-clipboard, cliphist, playerctl, brightnessctl, kitty"
  make_deb yami-files "$base" "ffmpegthumbnailer, libarchive-tools, udisks2, gvfs-backends, gvfs-fuse"
  make_deb yami-view "$base"
  make_deb yami-play "$base, libmpv2"
  make_deb yami-shot "$base, grim" "tesseract-ocr, tesseract-ocr-eng, zbar-tools"
  make_deb yami-greet "$base, greetd"
fi

# ── checksums and the signature ─────────────────────────────────────────────
LOOSE_FILES=()
if ((LOOSE)); then
  for m in yami-files yami-view yami-play yami-shot; do install -m755 "$T/bin/$m" "$OUT/$m"; LOOSE_FILES+=("$m"); done
fi
(cd "$OUT" && rm -f SHA256SUMS SHA256SUMS.sig && sha256sum "$ARCHIVE" $( ((DEB)) && printf '%s ' ./*.deb) ${LOOSE_FILES[@]+"${LOOSE_FILES[@]}"} >SHA256SUMS)
if [[ -n $KEY ]]; then
  ssh-keygen -q -Y sign -f "$KEY" -n yami-release "$OUT/SHA256SUMS"
  say "signed: SHA256SUMS.sig"
fi
say "ready: $OUT"
ls -la "$OUT"
