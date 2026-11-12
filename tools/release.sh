#!/usr/bin/env bash
# A release, for the owner only: users see nothing of it but the new version in settings → updates.
# Two halves. Everything is made here first and nothing leaves the machine (a failure, or --undo, puts it
# all back); then one question, and the release goes out in an order where nobody can catch it half done.
#
#   tools/release.sh X.Y.Z             make it here, then ask «publish now?»
#   tools/release.sh --push X.Y.Z      publish a release made here (now or later)
#   tools/release.sh --status [X.Y.Z]  what of it is here, what is on GitHub
#   tools/release.sh --undo X.Y.Z      take back a release made here and not published
#   tools/release.sh --publish X.Y.Z   the GitHub release of a pushed tag again (notes, binaries)
#   tools/release.sh --keygen          make the signing key and trust it in keys/allowed_signers
#
# Made here, in this order: the changelog section (both languages); yami-px signed at the commit the
# modules build against (a new patch version once its tag is public); each module that changed since its
# tag in modules.lock gets the release's version and a signed tag of it, the rest keep theirs; the
# shell's versions, modules.lock, flake.nix and flake.lock (yami-dev flake-lock, no nix needed); the ready
# binaries (tools/dist.sh: archive, .deb, single files, signed SHA256SUMS) and the packages' sums (Arch,
# Fedora, Gentoo); tools/check.sh; the commit «release: X.Y.Z» and its signed tag. The release key's
# passphrase is asked once, for an ssh-agent that lives as long as this script.
# Published: the new module tags (a repository is created on GitHub the first time), the shell's main and
# the yami-px tag, a draft GitHub release with the binaries, the tag vX.Y.Z, then the draft goes public.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
# shellcheck source=/dev/null
[[ -f $MODS/env.sh ]] && . "$MODS/env.sh"
OWNER="${YAMI_RELEASE_OWNER:-Trendorin}"
# tools/test-updates.sh (scenario «release») swaps these for stand-ins: the binaries, the tree check
DIST="${YAMI_RELEASE_DIST:-$ROOT/tools/dist.sh}"
CHECK="${YAMI_RELEASE_CHECK:-$ROOT/tools/check.sh --fast}"
RU=0
case "${LC_ALL:-${LC_MESSAGES:-${LANG:-}}}" in ru*) RU=1 ;; esac
t() { if ((RU)); then printf '%s' "$1"; else printf '%s' "$2"; fi; }
say() { printf '\e[38;2;196;22;28m▪\e[0m %s\n' "$*"; }
head_() { printf '\e[1m── %s\e[0m\n' "$*"; }
die() { printf '\e[31m✕ %s\e[0m\n' "$*" >&2; exit 1; }
KEYS=keys/allowed_signers
STATE_DIR="$(git rev-parse --git-common-dir)/yami-release"
dev() { cargo run --release -q -p yami-dev --manifest-path "$ROOT/shell/Cargo.toml" -- "$@"; }
modules() { awk '$1 !~ /^#/ && NF == 2 { print $1, $2 }' "$ROOT/modules.lock"; }
ver_ok() { [[ $1 =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; }
newer() { [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -n1)" == "$1" && $1 != "$2" ]]; }

# ── the key: one passphrase for the whole run ───────────────────────────────
PUB="" AGENT=0
key_init() {
  [[ -z $PUB ]] || return 0
  local k
  k="$(git config user.signingkey || true)"
  [[ -n $k ]] || die "$(t "нет ключа подписи: tools/release.sh --keygen" "no signing key: tools/release.sh --keygen")"
  k="${k/#\~/$HOME}"
  if [[ $k == *.pub ]]; then PUB=$k; elif [[ -f $k.pub ]]; then PUB=$k.pub; else die "$(t "нет открытой половины ключа: $k.pub" "the key's public half is missing: $k.pub")"; fi
  grep -qF "$(cut -d' ' -f2 "$PUB")" "$KEYS" || die "$(t "ключа $PUB нет в $KEYS: клиенты не примут выпуск" "$PUB is not in $KEYS: clients would refuse the release")"
  local pub
  pub="$(cut -d' ' -f1,2 "$PUB")"
  if [[ -n ${SSH_AUTH_SOCK:-} ]] && ssh-add -L 2>/dev/null | grep -qF "$pub"; then return 0; fi
  eval "$(ssh-agent -s)" >/dev/null
  AGENT=1
  trap agent_down EXIT
  say "$(t "пароль ключа выпуска — один раз на всё" "the release key's passphrase, once for everything")"
  # the passphrase from the terminal even when stdin is a pipe; no terminal (a test): as it comes
  if { : </dev/tty; } 2>/dev/null; then ssh-add -q "${PUB%.pub}" </dev/tty; else ssh-add -q "${PUB%.pub}"; fi || die "$(t "ключ не открылся" "the key did not open")"
}
# only the agent this script started (someone's own agent stays)
agent_down() { ((!AGENT)) || ssh-agent -k >/dev/null 2>&1 || true; }
# git with the release key (module repositories have no signing key of their own)
gs() { git -c gpg.format=ssh -c "user.signingkey=$PUB" -c gpg.ssh.allowedSignersFile="$ROOT/$KEYS" "$@"; }
signed_tag() { git -C "$1" cat-file -p "refs/tags/$2" 2>/dev/null | grep -q 'BEGIN SSH SIGNATURE'; }
verified() { gs -C "$1" tag -v "$2" >/dev/null 2>&1; }

# ── what was here before: --undo (and a failed run) puts exactly this back ──────
# one line per thing, fields split by tabs (the repository path holds a space): «repo DIR HEAD»,
# «tag DIR NAME OBJECT|-», «phase making|made|pushed»
state_file() { printf '%s/%s' "$STATE_DIR" "$1"; }
note() { local IFS=$'\t'; printf '%s\n' "$*" >>"$STATE"; }
note_tag() { # dir name: what the tag was before this release first touched it
  local obj
  obj="$(git -C "$1" rev-parse -q --verify "refs/tags/$2" || echo -)"
  awk -F'\t' -v d="$1" -v n="$2" '$1 == "tag" && $2 == d && $3 == n { f = 1 } END { exit !f }' "$STATE" || note tag "$1" "$2" "$obj"
}
phase() { awk -F'\t' '$1 == "phase" { p = $2 } END { print p }' "$1" 2>/dev/null; }

undo() { # version
  local v=$1 st kind a b c
  st="$(state_file "$v")"
  [[ -f $st ]] || die "$(t "выпуск $v здесь не делали" "release $v was not made here")"
  [[ "$(phase "$st")" != pushed ]] || die "$(t "$v уже опубликован: его не забрать, только выпустить следующий" "$v is published: it cannot be taken back, only followed by a new version")"
  if git ls-remote --exit-code --tags origin "refs/tags/v$v" >/dev/null 2>&1; then
    die "$(t "тег v$v уже на GitHub: его не забрать" "the tag v$v is on GitHub already: it cannot be taken back")"
  fi
  # after a finished run nothing may have been added on top: that work would be lost
  if [[ "$(phase "$st")" == made ]]; then
    while IFS=$'\t' read -r kind a b; do
      [[ $kind == repo ]] || continue
      [[ -z "$(git -C "$a" status --porcelain --untracked-files=no)" ]] || die "$(t "в $a незакоммиченные правки: сначала убери их" "$a has uncommitted edits: put them away first")"
    done <"$st"
  fi
  while IFS=$'\t' read -r kind a b c; do
    case $kind in
      repo) git -C "$a" reset -q --hard "$b" && say "$(t "назад" "back"): ${a##*/} ${b:0:8}" ;;
      tag) if [[ $c == - ]]; then git -C "$a" tag -d "$b" >/dev/null 2>&1 || true; else git -C "$a" update-ref "refs/tags/$b" "$c"; fi ;;
    esac
  done < <(tac "$st")
  # the binaries of a release that is not one
  local f
  while read -r f; do rm -f -- "$f"; done < <(assets "$v")
  rm -f -- "$st"
  say "$(t "выпуска $v здесь больше нет" "release $v is gone from here")"
}

# ── the changelog section and the GitHub release's text ───────────────────
section() { # file ver
  git show "v$2:$1" 2>/dev/null | awk -v h="## $2" '$0 == h { on = 1; next } on && /^## / { exit } on' | sed -e '/./,$!d' || true
}
notes() { # ver
  local v=$1 n ru
  n="$(section CHANGELOG.md "$v")"
  [[ -n $n ]] || n="- $(t "изменения — в истории коммитов" "see the commit history")"
  ru="$(section CHANGELOG.ru.md "$v")"
  [[ -z $ru ]] || n="$n

<details><summary>По-русски</summary>

$ru

</details>"
  cat <<EOF
$n

### Install

\`\`\`sh
bash <(curl -fsSL https://raw.githubusercontent.com/$OWNER/yami/v$v/install.sh)
\`\`\`

Already on yami: settings → updates → **update**, or \`yami update apply\`.

Ready binaries for x86_64 with glibc 2.34 or newer, no compiler needed; otherwise it builds from source (Arch, Fedora, Debian/Ubuntu, openSUSE, Void, Alpine, Gentoo, NixOS). Packages are attached below. The tag and SHA256SUMS are signed; check the tag with
\`git -c gpg.ssh.allowedSignersFile=keys/allowed_signers verify-tag v$v\`.

Описание и установка по-русски: [README.ru.md](https://github.com/$OWNER/yami/blob/v$v/README.ru.md).
EOF
}
assets() { # ver: the files attached to the release
  local v=$1 f
  for f in "yami-$v-x86_64-linux.tar.gz" yami-shell_"$v"_amd64.deb yami-files_"$v"_amd64.deb yami-view_"$v"_amd64.deb \
    yami-play_"$v"_amd64.deb yami-shot_"$v"_amd64.deb yami-greet_"$v"_amd64.deb yami-files yami-view yami-play yami-shot SHA256SUMS SHA256SUMS.sig; do
    printf '%s\n' "dist/$f"
  done
}
# the binaries in dist/ are the ones of this release and carry its signature
dist_ok() { # ver
  local v=$1 f who
  while read -r f; do [[ -s $f ]] || { echo "$f"; return 1; }; done < <(assets "$v")
  who="$(ssh-keygen -Y find-principals -f "$KEYS" -s dist/SHA256SUMS.sig 2>/dev/null | head -n1)"
  [[ -n $who ]] && ssh-keygen -q -Y verify -f "$KEYS" -I "$who" -n yami-release -s dist/SHA256SUMS.sig <dist/SHA256SUMS >/dev/null 2>&1 || { echo "SHA256SUMS.sig"; return 1; }
  (cd dist && sha256sum -c --quiet SHA256SUMS) >/dev/null 2>&1 || { echo "SHA256SUMS"; return 1; }
}
gh_ok() { command -v gh >/dev/null && gh auth status >/dev/null 2>&1; }

# the install counter (docs/SECURITY.md, «Install count»): one small file on one release that is never
# «latest», made once. install.sh downloads it once per machine; never re-upload it, that zeroes the count
counter_release() {
  gh release view installs >/dev/null 2>&1 && return 0
  local f
  f="$(mktemp -d)/yami-installs.txt"
  printf '%s\n' "yami install counter: install.sh downloads this file once per machine, so its download" \
    "count is the number of installs. Nothing is sent with it. See docs/SECURITY.md, «Install count»." >"$f"
  gh release create installs --title "install counter (not a release)" --notes "The download count of this file is the «installs» badge in the README. See docs/SECURITY.md." --prerelease --latest=false "$f" >/dev/null &&
    say "$(t "счётчик установок создан" "install counter created")" || say "$(t "счётчик не создался" "the counter was not created")"
  rm -rf "${f%/*}"
}
latest_of() { [[ "$(git tag -l 'v[0-9]*' --sort=-v:refname | head -n1)" == "v$1" ]] && echo true || echo false; }
# the GitHub release of an existing tag: its text, its binaries (from dist/, rebuilt at the tag when missing)
github_release() { # ver [--draft]
  local v=$1 draft=${2:-} up=()
  if ! dist_ok "$v" >/dev/null; then
    [[ "$(git rev-parse HEAD)" == "$(git rev-parse "v$v^{commit}")" ]] || die "$(t "в dist/ нет бинарей $v, а дерево не на v$v" "dist/ has no binaries of $v and the tree is not at v$v")"
    key_init
    SOURCE_DATE_EPOCH="$(git log -1 --format=%ct "v$v")" "$DIST" --deb --loose --strict --sign "$PUB" >/dev/null || die "tools/dist.sh"
  fi
  mapfile -t up < <(assets "$v")
  if gh release view "v$v" >/dev/null 2>&1; then
    gh release edit "v$v" --title "yami $v" --notes "$(notes "$v")" >/dev/null
  elif [[ $draft == --draft ]]; then
    gh release create "v$v" --draft --target "$(git rev-parse "v$v^{commit}")" --title "yami $v" --notes "$(notes "$v")" >/dev/null
  else
    gh release create "v$v" --verify-tag --title "yami $v" --notes "$(notes "$v")" --latest="$(latest_of "$v")" >/dev/null
  fi
  gh release upload "v$v" --clobber "${up[@]}" >/dev/null
}

# ── publishing a release made here ──────────────────────────────────────────
# the repository's description on GitHub: the README's first sentence, links as plain words
about() { awk 'NR > 3 && NF { p = p (p ? " " : "") $0; next } p { exit } END { print p }' "$1" | sed -E 's/\[([^]]*)\]\([^)]*\)/\1/g; s/([.!?]) .*/\1/' | cut -c1-300; }
remote_has() { # dir ref object: the ref is on origin and is that object
  local got
  got="$(git -C "$1" ls-remote origin "$2" 2>/dev/null | awk 'NR == 1 { print $1 }')"
  [[ -n $got ]] || return 1
  [[ $got == "$3" ]] || die "$(t "$2 на GitHub не тот, что здесь (${got:0:8} против ${3:0:8})" "$2 on GitHub is not the one here (${got:0:8} vs ${3:0:8})")"
}
push_release() { # ver
  local v=$1 st repo tag d obj px
  st="$(state_file "$v")"
  [[ "$(phase "$st")" == made ]] || die "$(t "выпуск $v здесь не готов (tools/release.sh --status $v)" "release $v is not made here (tools/release.sh --status $v)")"
  gh_ok || die "$(t "gh не вошёл: gh auth login" "gh is not logged in: gh auth login")"
  git rev-parse -q --verify "refs/tags/v$v" >/dev/null || die "$(t "нет тега v$v" "no tag v$v")"
  local bad
  bad="$(dist_ok "$v")" || die "$(t "бинари в dist/ не годятся" "the binaries in dist/ do not check out"): $bad"
  # 1. the modules: what the installer and Nix fetch at the shell's tag must already be there
  while read -r repo tag; do
    d="$MODS/$repo"
    obj="$(git -C "$d" rev-parse "refs/tags/$tag")"
    head_ "$repo $tag"
    if ! gh repo view "$OWNER/$repo" >/dev/null 2>&1; then
      gh repo create "$OWNER/$repo" --public --description "$(about "$d/README.md")" >/dev/null
      say "$(t "репозиторий создан" "repository created"): github.com/$OWNER/$repo"
    fi
    git -C "$d" remote get-url origin >/dev/null 2>&1 || git -C "$d" remote add origin "https://github.com/$OWNER/$repo.git"
    if remote_has "$d" "refs/tags/$tag" "$obj"; then
      say "$(t "уже на GitHub" "already on GitHub")"
    else
      git -C "$d" push -q origin "refs/tags/$tag^{commit}:refs/heads/main" "refs/tags/$tag"
      say "$(t "отправлен" "pushed")"
    fi
  done < <(modules)
  # 2. the shell's main and the tag of yami-px its modules build against
  px="px-v$(sed -n 's/^version = "\(.*\)"/\1/p' shell/px/Cargo.toml | head -n1)"
  head_ "yami main, $px"
  if remote_has "$ROOT" "refs/tags/$px" "$(git rev-parse "refs/tags/$px")"; then
    git push -q origin "refs/tags/v$v^{commit}:refs/heads/main"
  else
    git push -q origin "refs/tags/v$v^{commit}:refs/heads/main" "refs/tags/$px"
  fi
  # 3. the release as a draft with its binaries, 4. the tag (clients see the version from here on),
  # 5. the draft goes public
  head_ "yami $v"
  github_release "$v" --draft
  git push -q origin "refs/tags/v$v"
  gh release edit "v$v" --draft=false --latest="$(latest_of "$v")" >/dev/null
  counter_release
  note phase pushed
  say "$(t "опубликовано: yami $v у всех в настройках → обновления" "published: yami $v is in everyone's settings → updates")"
}

status() { # [ver]
  local v=${1:-} st repo tag d here gh_ out
  if [[ -z $v ]]; then
    v="$(ls -1 "$STATE_DIR" 2>/dev/null | grep -E '^[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n1 || true)"
    [[ -n $v ]] || { say "$(t "здесь нет сделанных выпусков; последний тег:" "no release made here; the last tag:") $(git tag -l 'v[0-9]*' --sort=-v:refname | head -n1)"; return 0; }
  fi
  st="$(state_file "$v")"
  case "$(phase "$st")" in
    making) say "yami $v: $(t "делается (или оборвался: --undo $v)" "being made (or cut short: --undo $v)")" ;;
    made) say "yami $v: $(t "сделан здесь, не опубликован" "made here, not published")" ;;
    pushed) say "yami $v: $(t "опубликован" "published")" ;;
    *) say "yami $v: $(t "здесь не делался" "not made here")" ;;
  esac
  where() { # dir ref
    here="$(git -C "$1" rev-parse -q --verify "$2" >/dev/null && t "здесь" "here" || t "нет" "no")"
    out="$(git -C "$1" ls-remote origin "$2" 2>/dev/null | awk 'NR == 1 { print "GitHub" }')"
    printf '  %-12s %-18s %-6s %s\n' "${1##*/}" "${2#refs/tags/}" "$here" "${out:-—}"
  }
  where "$ROOT" "refs/tags/v$v"
  where "$ROOT" "refs/tags/px-v$(sed -n 's/^version = "\(.*\)"/\1/p' shell/px/Cargo.toml | head -n1)"
  while read -r repo tag; do where "$MODS/$repo" "refs/tags/$tag"; done < <(modules)
  if gh_ok; then
    gh_="$(gh release view "v$v" --json isDraft,assets --jq '(if .isDraft then "draft" else "public" end) + ", " + (.assets | length | tostring) + " files"' 2>/dev/null || echo "—")"
    printf '  %-12s %s\n' "release" "$gh_"
  fi
  dist_ok "$v" >/dev/null && printf '  %-12s %s\n' "dist/" "$(t "бинари $v, подпись верна" "binaries of $v, signature checks out")" || printf '  %-12s %s\n' "dist/" "—"
}

# ── making a release here ───────────────────────────────────────────────────
px_ver() { sed -n 's/^version = "\(.*\)"/\1/p' shell/px/Cargo.toml | head -n1; }
px_of() { grep -A2 '^name = "yami-px"$' "$1/Cargo.lock" 2>/dev/null | sed -n 's/.*#\([0-9a-f]*\)".*/\1/p'; }
crate_ver() { # file version: only the [package] version (the file's first `version =` line)
  sed -i "0,/^version = \".*\"/s//version = \"$2\"/" "$1"
}

make_release() { # ver
  local v=$1 cur
  ver_ok "$v" || die "$(t "как: tools/release.sh X.Y.Z (--help)" "usage: tools/release.sh X.Y.Z (--help)")"
  cur="$(cat VERSION)"
  newer "$v" "$cur" || die "$(t "$v не новее $cur" "$v is not newer than $cur")"
  [[ "$(git rev-parse --abbrev-ref HEAD)" == main ]] || die "$(t "выпуск — только с main" "release from main only")"
  [[ -z "$(git status --porcelain --untracked-files=no)" ]] || die "$(t "есть незакоммиченные правки" "uncommitted changes")"
  git rev-parse -q --verify "refs/tags/v$v" >/dev/null && die "$(t "тег v$v уже есть" "tag v$v exists")"
  [[ ! -f "$(state_file "$v")" ]] || die "$(t "выпуск $v уже начат здесь: tools/release.sh --status $v (или --undo $v)" "release $v is begun here: tools/release.sh --status $v (or --undo $v)")"
  local repo tag d
  while read -r repo tag; do
    d="$MODS/$repo"
    [[ -d $d/.git ]] || die "$(t "нет модуля" "no module") $d"
    [[ -z "$(git -C "$d" status --porcelain --untracked-files=no)" ]] || die "$repo: $(t "незакоммиченные правки" "uncommitted changes")"
    [[ "$(git -C "$d" rev-parse --abbrev-ref HEAD)" == main ]] || die "$repo: $(t "не на main" "not on main")"
  done < <(modules)
  git ls-remote origin HEAD >/dev/null 2>&1 || die "$(t "GitHub не отвечает: нужно знать, что там уже есть" "GitHub does not answer: the release needs to know what is there")"
  dev check-docs >/dev/null || dev check-docs || die "$(t "документы EN и RU расходятся (yami-dev check-docs)" "the EN and RU docs differ (yami-dev check-docs)")"
  key_init

  mkdir -p "$STATE_DIR"
  STATE="$(state_file "$v")"
  : >"$STATE"
  note phase making
  note repo "$ROOT" "$(git rev-parse HEAD)"
  while read -r repo tag; do note repo "$MODS/$repo" "$(git -C "$MODS/$repo" rev-parse HEAD)"; done < <(modules)
  # any failure from here on puts everything back (REL: the trap runs after this function's locals are gone)
  REL=$v
  trap 'rc=$?; if ((rc)) && [[ "$(phase "$STATE")" == making ]]; then printf "\e[31m✕ %s\e[0m\n" "$(t "не вышло: всё возвращаю как было" "it failed: putting everything back")" >&2; undo "$REL"; fi; agent_down' EXIT

  head_ "$(t "что изменилось" "what changed")"
  # «Unreleased» («Не выпущено») becomes the release in both files; both must have lines there
  dev changelog "$v" || die "$(t "в «Unreleased» CHANGELOG.md или «Не выпущено» CHANGELOG.ru.md пусто" "«Unreleased» in CHANGELOG.md or «Не выпущено» in CHANGELOG.ru.md is empty")"
  if [[ -t 0 && -n ${EDITOR:-} ]]; then "$EDITOR" CHANGELOG.md CHANGELOG.ru.md; fi

  # ── yami-px: the commit the modules build against, signed ─────────────
  local px pxt pxc changed=1 on_gh=0
  px="$(px_ver)" pxt="px-v$(px_ver)"
  head_ "yami-px $pxt"
  git ls-remote --exit-code --tags origin "refs/tags/$pxt" >/dev/null 2>&1 && on_gh=1
  note_tag "$ROOT" "$pxt"
  # a clone without the tag GitHub has: take it from there
  if ((on_gh)) && ! git rev-parse -q --verify "refs/tags/$pxt" >/dev/null; then git fetch -q origin "refs/tags/$pxt:refs/tags/$pxt"; fi
  if git rev-parse -q --verify "refs/tags/$pxt" >/dev/null; then
    git diff --quiet "$pxt^{commit}" HEAD -- shell/px && changed=0
  fi
  if ((changed && on_gh)); then
    # a public tag never moves: the next patch version
    local n="${px%.*}.$((${px##*.} + 1))"
    crate_ver shell/px/Cargo.toml "$n"
    cargo update -q -p yami-px --offline --manifest-path shell/Cargo.toml
    git add shell/px/Cargo.toml shell/Cargo.lock
    git commit -q -m "px: $n"
    pxt="px-v$n" on_gh=0
    note_tag "$ROOT" "$pxt"
    say "$px → $n"
  fi
  # not public yet: signed at the commit (moved to this one when px changed since)
  if ((changed)); then
    gs tag -f -s "$pxt" -m "yami-px ${pxt#px-v}" HEAD >/dev/null
  elif ((!on_gh)) && ! signed_tag "$ROOT" "$pxt"; then
    gs tag -f -s "$pxt" -m "yami-px ${pxt#px-v}" "$(git rev-parse "$pxt^{commit}")" >/dev/null
  fi
  pxc="$(git rev-parse "$pxt^{commit}")"
  # a public tag is what it is (it cannot change); one signed here must check out
  ((on_gh)) || verified "$ROOT" "$pxt" || die "$pxt: $(t "подпись не сходится" "the signature does not check out")"
  say "$pxt → ${pxc:0:8}"

  # ── the modules: a changed one gets this version and a signed tag ──────
  local lock_new="" t2 cv
  while read -r repo tag; do
    d="$MODS/$repo"
    head_ "$repo"
    # built against another yami-px: take this one
    if [[ -f $d/Cargo.lock && -n "$(px_of "$d")" && "$(px_of "$d")" != "$pxc" ]]; then
      sed -i "s|\(yami-px = {[^}]*tag = \"\)px-v[^\"]*\"|\1$pxt\"|" "$d/Cargo.toml"
      (cd "$d" && cargo update -q -p yami-px)
      [[ "$(px_of "$d")" == "$pxc" ]] || die "$repo: $(t "не взял" "did not take") $pxt"
      git -C "$d" add Cargo.toml Cargo.lock
      git -C "$d" commit -q -m "take yami-px $pxt"
      say "yami-px → $pxt"
    fi
    t2=$tag
    if ! git -C "$d" rev-parse -q --verify "refs/tags/$tag" >/dev/null || [[ -n "$(git -C "$d" rev-list "$tag..HEAD")" ]]; then
      t2="v$v"
      git -C "$d" rev-parse -q --verify "refs/tags/$t2" >/dev/null && [[ "$(git -C "$d" rev-parse "$t2^{commit}")" != "$(git -C "$d" rev-parse HEAD)" ]] &&
        die "$repo: $(t "тег $t2 уже есть и указывает не туда" "the tag $t2 exists and points elsewhere")"
      if [[ -f $d/Cargo.toml ]]; then
        cv="$(sed -n 's/^version = "\(.*\)"/\1/p' "$d/Cargo.toml" | head -n1)"
        if [[ $cv != "$v" ]]; then
          crate_ver "$d/Cargo.toml" "$v"
          (cd "$d" && cargo update -q -p "$repo" --offline)
          git -C "$d" add Cargo.toml Cargo.lock
          git -C "$d" commit -q -m "release: $v"
        fi
      fi
      note_tag "$d" "$t2"
      gs -C "$d" tag -f -s "$t2" -m "$repo $t2" >/dev/null
      verified "$d" "$t2" || die "$repo $t2: $(t "подпись не сходится" "the signature does not check out")"
      say "$t2 → $(git -C "$d" rev-parse --short=8 HEAD)"
    else
      signed_tag "$d" "$tag" || die "$repo $tag: $(t "тег без подписи — установщик его не примет" "the tag is unsigned: the installer would refuse it")"
      say "$(t "не менялся" "unchanged"): $tag"
    fi
    lock_new+="$repo $t2"$'\n'
  done < <(modules)
  printf '%s' "$lock_new" >modules.lock
  # the flake's inputs follow modules.lock, flake.lock their tags (yami-dev computes what nix would)
  while read -r repo tag; do
    sed -i "s|url = \"github:$OWNER/$repo/[^\"]*\";|url = \"github:$OWNER/$repo/$tag\";|" flake.nix
  done < <(modules)
  dev flake-lock "$MODS" >/dev/null || die "yami-dev flake-lock"

  # ── the shell's versions ──────────────────────────────────────────────
  head_ "yami $cur → $v"
  echo "$v" >VERSION
  local crates=(shell/Cargo.toml shell/*/Cargo.toml) f
  for f in "${crates[@]}"; do
    [[ $f == shell/px/Cargo.toml ]] || crate_ver "$f" "$v"
  done
  cargo update -q --workspace --offline --manifest-path shell/Cargo.toml

  # ── the ready binaries, then the packages' sums: the commit gets the time the archive carries ──
  local epoch archive
  epoch="$(date +%s)"
  head_ "$(t "готовые бинари" "ready binaries")"
  rm -f dist/SHA256SUMS dist/SHA256SUMS.sig
  SOURCE_DATE_EPOCH=$epoch "$DIST" --deb --loose --strict --sign "$PUB" >"$STATE_DIR/dist.log" 2>&1 || { tail -n 30 "$STATE_DIR/dist.log"; die "tools/dist.sh"; }
  grep -E '^  yami-[a-z]+ +[0-9]+ KiB' "$STATE_DIR/dist.log" || true
  dist_ok "$v" >/dev/null || die "tools/dist.sh"
  archive="dist/yami-$v-x86_64-linux.tar.gz"
  sed -i -e "s/^pkgver=.*/pkgver=$v/" -e "s/^pkgrel=.*/pkgrel=1/" \
    -e "s/^sha256sums=.*/sha256sums=('$(sha256sum "$archive" | cut -d' ' -f1)') # tools\/release.sh writes it/" packaging/arch/PKGBUILD
  sed -i -e "s/^\(Version:[[:space:]]*\).*/\1$v/" -e "s/^\(Release:[[:space:]]*\).*/\11%{?dist}/" packaging/fedora/yami.spec
  local old
  old="$(git ls-files 'packaging/gentoo/yami-bin-*.ebuild' | head -n1)"
  [[ $old == "packaging/gentoo/yami-bin-$v.ebuild" ]] || git mv "$old" "packaging/gentoo/yami-bin-$v.ebuild"
  printf 'DIST %s %s BLAKE2B %s SHA512 %s\n' "${archive##*/}" "$(stat -c %s "$archive")" "$(b2sum "$archive" | cut -d' ' -f1)" "$(sha512sum "$archive" | cut -d' ' -f1)" >packaging/gentoo/Manifest
  git add packaging
  say "PKGBUILD, yami.spec, yami-bin-$v.ebuild + Manifest"

  head_ "tools/check.sh"
  $CHECK >"$STATE_DIR/check.log" 2>&1 || { tail -n 20 "$STATE_DIR/check.log"; die "tools/check.sh"; }
  say "check: all good"

  git add VERSION CHANGELOG.md CHANGELOG.ru.md modules.lock flake.nix flake.lock shell/Cargo.lock "${crates[@]}"
  GIT_AUTHOR_DATE="@$epoch +0000" GIT_COMMITTER_DATE="@$epoch +0000" git commit -q -m "release: $v"
  note_tag "$ROOT" "v$v"
  gs tag -s "v$v" -m "yami $v" >/dev/null
  verified "$ROOT" "v$v" || die "v$v: $(t "подпись не сходится" "the signature does not check out")"
  note phase made
  say "$(t "готово здесь: тег v$v подписан, ничего не отправлено" "made here: tag v$v signed, nothing sent")"
  echo
  status "$v"
  echo
  if [[ -t 0 ]]; then
    local a
    read -r -p "$(t "опубликовать сейчас? [y/N] " "publish now? [y/N] ")" a
    if [[ $a == [yYдД]* ]]; then push_release "$v"; return 0; fi
  fi
  say "$(t "потом: tools/release.sh --push $v · забрать назад: --undo $v" "later: tools/release.sh --push $v · take it back: --undo $v")"
}

# ── commands ────────────────────────────────────────────────────────────────
case ${1:-} in
  -h | --help) sed -n '2,/^set -euo/p' "$0" | sed -e '$d' -e 's/^# \{0,1\}//'; exit 0 ;;
  --push) ver_ok "${2:-}" || die "tools/release.sh --push X.Y.Z"; STATE="$(state_file "$2")"; push_release "$2" ;;
  --undo) ver_ok "${2:-}" || die "tools/release.sh --undo X.Y.Z"; undo "$2" ;;
  --status) status "${2:-}" ;;
  --publish)
    ver_ok "${2:-}" || die "tools/release.sh --publish X.Y.Z"
    gh_ok || die "$(t "gh не вошёл: gh auth login" "gh is not logged in: gh auth login")"
    git ls-remote --exit-code --tags origin "refs/tags/v$2" >/dev/null 2>&1 || die "$(t "тега v$2 нет на GitHub" "the tag v$2 is not on GitHub")"
    github_release "$2"
    counter_release
    say "$(t "релиз на GitHub: yami $2" "GitHub release: yami $2")" ;;
  --keygen)
    key="$HOME/.ssh/yami-release"
    [[ -f $key ]] || ssh-keygen -t ed25519 -C "yami release" -f "$key"
    git config gpg.format ssh
    git config user.signingkey "$key.pub"
    who="$(git config user.email || git config user.name)"
    grep -qF "$(cut -d' ' -f2 "$key.pub")" "$KEYS" ||
      printf '%s namespaces="git,yami-release" %s\n' "$who" "$(cut -d' ' -f1,2 "$key.pub")" >>"$KEYS"
    say "$(t "ключ: $key (храни его; без него выпуски не подписать)" "key: $key (keep it safe; releases cannot be signed without it)")"
    say "$(t "закоммить $KEYS: с версии, где он есть, ставятся только подписанные выпуски" "commit $KEYS: from the version that has it, only signed releases are installed")" ;;
  *) make_release "${1:-}" ;;
esac
