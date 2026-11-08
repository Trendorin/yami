# Модули yami живут в своих репозиториях; какая версия подходит этой версии шелла — в modules.lock
# («repo tag» по строке). Здесь: где взять исходники модуля и как проверить подпись тега.
# YAMI_MODULES_DIR — папка с уже готовыми checkout'ами (разработка, сборка без сети).
# shellcheck shell=bash

YAMI_MODULES_URL="${YAMI_MODULES_URL:-https://github.com/Trendorin}"

# компонент → репозиторий; без репозитория — пакеты системы, а не наш модуль
m_repo() {
  case $1 in
    files) echo yami-files ;;
    view) echo yami-view ;;
    play) echo yami-play ;;
    shot) echo yami-shot ;;
    agents) echo yami-agents ;;
    login) echo yami-greet ;;
    fork) echo yami-niri ;;
    *) return 1 ;;
  esac
}
m_tag() { awk -v r="$1" '$1 == r { print $2 }' "$ROOT/modules.lock" 2>/dev/null; }

# исходники модуля: печатает папку; нет сети или подписи — код возврата не 0
m_fetch() {
  local repo=$1 tag dir
  if [[ -n ${YAMI_MODULES_DIR:-} && -d $YAMI_MODULES_DIR/$repo ]]; then
    printf '%s\n' "$YAMI_MODULES_DIR/$repo"
    return 0
  fi
  tag="$(m_tag "$repo")"
  [[ -n $tag ]] || { echo "modules.lock: no version for $repo" >&2; return 1; }
  dir="${XDG_DATA_HOME:-$HOME/.local/share}/yami/modules/$repo"
  if [[ -d $dir/.git ]]; then
    git -C "$dir" fetch --quiet --tags origin >&2 || return 1
  else
    mkdir -p "${dir%/*}"
    git clone --quiet --filter=blob:none "$YAMI_MODULES_URL/$repo.git" "$dir" >&2 || return 1
  fi
  # подпись проверяется ключами из этой копии шелла: подмена модуля не пройдёт вместе с ним
  if ! git -C "$dir" -c gpg.format=ssh -c "gpg.ssh.allowedSignersFile=$ROOT/keys/allowed_signers" verify-tag "$tag" >/dev/null 2>&1; then
    echo "$repo $tag: the signature does not check out" >&2
    return 1
  fi
  git -C "$dir" checkout --quiet --detach "$tag" >&2 || return 1
  printf '%s\n' "$dir"
}
