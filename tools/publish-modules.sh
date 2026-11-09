#!/usr/bin/env bash
# Put the modules on GitHub: each repository with the signed tag modules.lock names (yami-px with the tag
# the shell's Cargo.toml names). Run by hand before the first release that needs them; it asks for the
# release key's passphrase. Re-running is safe: a repository that exists is only pushed to.
#   tools/publish-modules.sh [--dry-run]     --dry-run prints what it would do and changes nothing
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
OWNER=Trendorin
DRY=0
[[ ${1:-} == --dry-run ]] && DRY=1
run() { if ((DRY)); then printf '  %s\n' "$*"; else "$@"; fi; }
die() { printf '\e[31m✕ %s\e[0m\n' "$*" >&2; exit 1; }
KEY="$(git -C "$ROOT" config user.signingkey || true)"
[[ -n $KEY ]] || die "no signing key: tools/release.sh --keygen"
command -v gh >/dev/null && gh auth status >/dev/null 2>&1 || die "gh is not logged in"

publish() { # repository tag
  local repo=$1 tag=$2 d="$MODS/$1"
  [[ -d $d/.git ]] || die "no $d"
  [[ -z "$(git -C "$d" status --porcelain --untracked-files=no)" ]] || die "$repo has uncommitted edits"
  printf '\e[1m── %s %s\e[0m\n' "$repo" "$tag"
  gh repo view "$OWNER/$repo" >/dev/null 2>&1 || run gh repo create "$OWNER/$repo" --public --description "$repo: a module of the yami desktop for niri"
  git -C "$d" remote get-url origin >/dev/null 2>&1 || run git -C "$d" remote add origin "https://github.com/$OWNER/$repo.git"
  run git -C "$d" -c gpg.format=ssh -c "user.signingkey=$KEY" tag -f -s "$tag" -m "$repo $tag"
  run git -C "$d" push origin "HEAD:refs/heads/main" "refs/tags/$tag"
}

px_tag="$(sed -n 's/.*yami-px.*tag *= *"\([^"]*\)".*/\1/p' "$ROOT/shell/Cargo.toml" | head -n1)"
[[ -n $px_tag ]] || die "no tag of yami-px in shell/Cargo.toml"
publish yami-px "$px_tag"
while read -r repo tag; do
  [[ -n $repo ]] && publish "$repo" "$tag"
done <"$ROOT/modules.lock"
if command -v nix >/dev/null; then
  run nix flake lock "$ROOT"
else
  printf 'no nix here: run `nix flake lock` once somewhere, the flake needs the modules in flake.lock\n'
fi
