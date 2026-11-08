#!/usr/bin/env bash
# Ready files of the modules, one binary each, into ../yami-modules/dist with SHA256SUMS. Publishing
# them is a separate decision. Prints the newest glibc each file needs: it runs on systems at least that new.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODS="${YAMI_MODULES_DIR:-$ROOT/../yami-modules}"
# shellcheck source=/dev/null
[[ -f $MODS/env.sh ]] && . "$MODS/env.sh"
OUT="$MODS/dist"
rm -rf -- "$OUT"
mkdir -p "$OUT"
for m in yami-files yami-view yami-play yami-shot yami-agents; do
  (cd "$MODS/$m" && cargo build --release --locked)
  install -m755 "$MODS/$m/target/release/$m" "$OUT/$m"
  glibc="$(objdump -T "$OUT/$m" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -n1)"
  printf '%-12s %8s KiB  %s\n' "$m" "$(($(stat -c %s "$OUT/$m") / 1024))" "$glibc"
done
(cd "$OUT" && sha256sum yami-* >SHA256SUMS)
echo "dist-modules: $OUT"
