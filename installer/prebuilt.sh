# Ready binaries instead of a compiler. A release carries one archive (tools/dist.sh) that runs on every
# glibc distribution: downloading it takes a minute where a build takes minutes and gigabytes. Used when
# this tree is exactly a release, the archive is there and its signature checks out against
# keys/allowed_signers of this tree; anything else (another CPU, musl, the development branch,
# --from-source) goes on to the build as before.
# shellcheck shell=bash

PREBUILT=0 PREBUILT_DIR="" PREBUILT_ASSET=""
# the newest glibc the archive asks for
GLIBC_NEED=2.34

prebuilt_where() { printf '%s' "${PREBUILT_FROM:-${YAMI_PREBUILT_URL:-https://github.com/Trendorin/yami/releases/download/v$(cat "$ROOT/VERSION")}}"; }
glibc_ok() {
  local v
  v="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
  [[ -n $v ]] && ver_ge "$v" "$GLIBC_NEED"
}
# the file at the release's address, or a folder given with --prebuilt
prebuilt_has() { # file
  local from
  from="$(prebuilt_where)"
  if [[ $from == *://* ]]; then
    curl -fsSIL --proto '=https,file' --connect-timeout 5 --max-time 10 -o /dev/null "$from/$1" 2>/dev/null
  else
    [[ -f $from/$1 ]]
  fi
}
prebuilt_get() { # file, where to put it
  local from
  from="$(prebuilt_where)"
  if [[ $from == *://* ]]; then
    curl -fsSL --proto '=https,file' --connect-timeout 10 --retry 2 -o "$2" "$from/$1"
  else
    cp -- "$from/$1" "$2"
  fi
}
# decided before anything else: the package list depends on it (no compiler when nothing is compiled)
prebuilt_init() {
  PREBUILT=0
  ((FROM_SOURCE || UNINSTALL)) && return 0
  local asked=0
  [[ -n $PREBUILT_FROM ]] && asked=1
  # checkouts of the modules next to this tree are built from what is in them
  ((asked)) || [[ -z ${YAMI_MODULES_DIR:-} ]] || return 0
  if [[ "$(uname -m)" != x86_64 ]] || ! glibc_ok; then
    ((asked)) && { echo "--prebuilt: needs x86_64 and glibc $GLIBC_NEED or newer" >&2; exit 2; }
    return 0
  fi
  local ver
  ver="$(cat "$ROOT/VERSION")"
  # the archive is built from a release: another commit or a tree with edits is built from itself
  if ((!asked)); then
    [[ "$(git -C "$ROOT" describe --tags --exact-match --match 'v[0-9]*' HEAD 2>/dev/null)" == "v$ver" && -n "$(src_rev "$ROOT")" ]] || return 0
  fi
  PREBUILT_ASSET="yami-$ver-x86_64-linux.tar.gz"
  if ! prebuilt_has "$PREBUILT_ASSET"; then
    ((asked)) && { echo "--prebuilt: no $PREBUILT_ASSET at $(prebuilt_where)" >&2; exit 2; }
    return 0
  fi
  PREBUILT=1
  # known up front: the steps run in subshells and share nothing but files
  PREBUILT_DIR="$CACHE/prebuilt/${PREBUILT_ASSET%.tar.gz}"
}
# the compiler is still wanted for yami-niri, or for niri itself when the packaged one is too old
needs_compiler() { ((!PREBUILT)) || on fork || { ((!APPLY)) && on core && ! niri_will_do && ((!DRY)); }; }

# the checksum file signed by a release key, then the archive against it. Keys in the tree: no signature, no install
prebuilt_verify() { # folder
  local d=$1 keys="$ROOT/keys/allowed_signers" who
  if grep -qvE '^[[:space:]]*(#|$)' "$keys" 2>/dev/null; then
    [[ -s $d/SHA256SUMS.sig ]] || { echo "the ready binaries of this release carry no signature"; return 1; }
    who="$(ssh-keygen -Y find-principals -f "$keys" -s "$d/SHA256SUMS.sig" 2>/dev/null | head -n1)"
    if [[ -z $who ]] || ! ssh-keygen -q -Y verify -f "$keys" -I "$who" -n yami-release -s "$d/SHA256SUMS.sig" <"$d/SHA256SUMS" >/dev/null 2>&1; then
      echo "the signature of the ready binaries does not check out"
      return 1
    fi
  fi
  (cd "$d" && sha256sum -c --ignore-missing --quiet SHA256SUMS) || { echo "the ready binaries do not match their checksum"; return 1; }
}
prebuilt_fetch() {
  local dl="$CACHE/prebuilt-dl" ex="$CACHE/prebuilt"
  rm -rf -- "$dl" "$ex"
  mkdir -p "$dl" "$ex"
  echo "$PREBUILT_ASSET"
  prebuilt_get "$PREBUILT_ASSET" "$dl/$PREBUILT_ASSET" || { echo "could not download $PREBUILT_ASSET"; return 1; }
  prebuilt_get SHA256SUMS "$dl/SHA256SUMS" || { echo "could not download SHA256SUMS"; return 1; }
  prebuilt_get SHA256SUMS.sig "$dl/SHA256SUMS.sig" 2>/dev/null || rm -f -- "$dl/SHA256SUMS.sig"
  prebuilt_verify "$dl" || return 1
  tar -xzf "$dl/$PREBUILT_ASSET" --no-same-owner -C "$ex" || return 1
  rm -rf -- "$dl"
  [[ -x $PREBUILT_DIR/bin/yami-shell ]] || { echo "the archive holds no yami-shell"; return 1; }
}
# a module's files (.desktop, portals, the login screen's scripts) stay where a git checkout of it would:
# settings → updates runs the login screen's script later from there
prebuilt_module() { # repository
  local dest="${XDG_DATA_HOME:-$HOME/.local/share}/yami/modules/$1"
  rm -rf -- "$dest"
  mkdir -p "${dest%/*}"
  cp -a -- "$PREBUILT_DIR/modules/$1" "$dest" || return 1
  [[ $1 != yami-greet ]] || install -m755 "$PREBUILT_DIR/bin/yami-greet" "$dest/yami-greet" || return 1
  printf '%s\n' "$dest" >"$CACHE/src-$1"
}
# the chosen binaries go to the same place a build leaves them; do_install_bins swaps them in
prebuilt_take() { # binary
  install -Dm755 "$PREBUILT_DIR/bin/$1" "$CACHE/target/release/$1" || return 1
  printf 'prebuilt-%s\n' "$(cat "$ROOT/VERSION")" >"$CACHE/built-$1"
}
do_prebuilt_bins() {
  local id repo
  # the services come with the shell
  ((APPLY)) || { prebuilt_take yami-shell && prebuilt_take yami-agents; } || return 1
  for id in files view play shot; do
    on "$id" || continue
    repo="$(m_repo "$id")" && prebuilt_take "$repo" || return 1
  done
  if wants login; then install -Dm755 "$PREBUILT_DIR/bin/yami-greet" "$CACHE/target/release/yami-greet" || return 1; fi
  rm -rf -- "$CACHE/prebuilt"
  return 0
}
