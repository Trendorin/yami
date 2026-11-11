#!/usr/bin/env bash
# Put the modules on GitHub: each repository with the signed tag modules.lock names. The drawing library
# they share, yami-px, lives in this repository (shell/px) and is published as its tag px-vX.Y.Z (the
# version of shell/px/Cargo.toml), signed and pushed to origin first: the modules' Cargo.lock names that
# commit. Run by hand before the first release that needs them; it asks for the release key's passphrase.
# Re-running is safe: a repository that exists is only pushed to, a tag already on GitHub is left alone.
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

# ── yami-px: the tag of this repository the modules build against ─────────
px_ver="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/shell/px/Cargo.toml" | head -n1)"
px_tag="px-v$px_ver"
printf '\e[1m── yami-px %s (this repository)\e[0m\n' "$px_tag"
px_commit="$(git -C "$ROOT" rev-parse -q --verify "refs/tags/$px_tag^{commit}")" || die "no tag $px_tag here: git tag $px_tag at the commit the modules lock"
git -C "$ROOT" fetch -q origin main || die "could not reach origin"
git -C "$ROOT" merge-base --is-ancestor "$px_commit" origin/main || die "$px_tag points at a commit that is not on origin/main yet: push main first"
if git -C "$ROOT" ls-remote --exit-code --tags origin "refs/tags/$px_tag" >/dev/null 2>&1; then
  echo "  already on GitHub"
else
  # signed at the same commit: the modules' Cargo.lock stays as it is
  run git -C "$ROOT" -c gpg.format=ssh -c "user.signingkey=$KEY" tag -f -s "$px_tag" -m "yami-px $px_ver" "$px_commit"
  run git -C "$ROOT" push origin "refs/tags/$px_tag"
fi

# ── the modules ─────────────────────────────────────────────────────────────
publish() { # repository tag
  local repo=$1 tag=$2 d="$MODS/$1"
  [[ -d $d/.git ]] || die "no $d"
  [[ -z "$(git -C "$d" status --porcelain --untracked-files=no)" ]] || die "$repo has uncommitted edits"
  if [[ -f $d/Cargo.lock ]] && grep -q 'name = "yami-px"' "$d/Cargo.lock"; then
    grep -A2 'name = "yami-px"' "$d/Cargo.lock" | grep -q "#$px_commit\"" || die "$repo: its Cargo.lock does not build against $px_tag (cargo update -p yami-px)"
  fi
  printf '\e[1m── %s %s\e[0m\n' "$repo" "$tag"
  gh repo view "$OWNER/$repo" >/dev/null 2>&1 || run gh repo create "$OWNER/$repo" --public --description "$(sed -n '3p' "$d/README.md" | cut -c1-300)"
  git -C "$d" remote get-url origin >/dev/null 2>&1 || run git -C "$d" remote add origin "https://github.com/$OWNER/$repo.git"
  run git -C "$d" -c gpg.format=ssh -c "user.signingkey=$KEY" tag -f -s "$tag" -m "$repo $tag"
  run git -C "$d" push origin "HEAD:refs/heads/main" "refs/tags/$tag"
}

while read -r repo tag; do
  [[ -n $repo && $repo != \#* ]] && publish "$repo" "$tag"
done <"$ROOT/modules.lock"
if command -v nix >/dev/null; then
  run nix flake lock "$ROOT"
else
  printf 'no nix here: run `nix flake lock` once somewhere, the flake needs the modules in flake.lock\n'
fi
