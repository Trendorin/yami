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

# папка исходников, записанная шагом «исходники» (do_fetch_module)
m_src() { [[ -s $CACHE/src-$1 ]] && cat "$CACHE/src-$1"; }
do_fetch_module() {
  local dir
  dir="$(m_fetch "$1")" || return 1
  printf '%s\n' "$dir" >"$CACHE/src-$1"
}
# выбранные модули: одной строкой на экране; дальше root-шаги и сборка берут всё из готовых папок
fetch_list() {
  local id repo
  for id in files view play shot agents login fork; do
    wants "$id" || continue
    repo="$(m_repo "$id")" && printf '%s\n' "$repo"
  done
}
do_fetch_modules() {
  local repo
  ((PREBUILT)) && { prebuilt_fetch || return 1; }
  for repo in $(fetch_list); do
    echo "$repo"
    # готовые бинари: файлы модуля — из того же архива; yami-niri собирается из исходников
    if ((PREBUILT)) && [[ -d $PREBUILT_DIR/modules/$repo ]]; then
      prebuilt_module "$repo" || return 1
    else
      do_fetch_module "$repo" || return 1
    fi
  done
}
plan_fetch() {
  if ((PREBUILT)); then
    add "$(t "готовые бинари: скачать и проверить подпись" "ready binaries: download and check the signature" "binários prontos: baixar e verificar a assinatura")" do_fetch_modules
  elif [[ -n "$(fetch_list)" ]]; then
    add "$(t "исходники модулей" "module sources" "código-fonte dos módulos")" do_fetch_modules
  fi
  return 0
}

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
  # тег — только по полному имени, а дальше по номерам объектов: голое имя «v3.0.0» подходит и ветке
  # (свежий клон получает ветку по умолчанию с сервера как свою), а `git checkout ИМЯ` берёт ветку
  # раньше тега — подпись тега сошлась бы, а собрался бы чужой код из одноимённой ветки
  local obj commit out
  if ! obj="$(git -C "$dir" rev-parse -q --verify "refs/tags/$tag")" || ! commit="$(git -C "$dir" rev-parse -q --verify "$obj^{commit}")"; then
    echo "$repo $tag: no such version in its repository" >&2
    return 1
  fi
  # подпись проверяется ключами из этой копии шелла: подмена модуля не пройдёт вместе с ним. Засчитывается
  # только подпись ключом из списка («for ИМЯ»): как в обновлениях шелла (update.rs, verify)
  if ! out="$(LC_ALL=C git -C "$dir" -c gpg.format=ssh -c "gpg.ssh.allowedSignersFile=$ROOT/keys/allowed_signers" -c gpg.minTrustLevel=fully verify-tag "$obj" 2>&1)" ||
    [[ $out != *'Good "git" signature for '* || $out == *'No principal matched'* ]]; then
    echo "$repo $tag: the signature does not check out" >&2
    return 1
  fi
  # собирается ровно тот коммит, на который указывает проверенный тег
  git -C "$dir" -c advice.detachedHead=false checkout --quiet --detach "$commit" >&2 || return 1
  printf '%s\n' "$dir"
}
