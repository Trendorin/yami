# What can be installed, each part on its own: a default for this machine (detect.sh), a flag
# (--no-ID / --ID, --shell-only), the first-run setup. An update repeats the choice of
# the first install (install.json, written by `yami update stamp`). What yami restyles in other
# programs is the other half of the choice (restyle.sh); `look` (the Qt/GTK theme packages) follows it.
# shellcheck shell=bash

# the order of the components page; look is not on it (restyle.sh sets it)
C_ID=(core files view play shot login games fork look)
# groups of packages that used to be components: their packages now come with core (apps, the night
# light) or with what uses them (drives with files and the shell, text and QR with shot); agents: the
# services (yami-agents) are part of the shell since 3.0, built and installed with it
C_RETIRED=" apps drives tools agents "
# components this installer offered last time: one that came later starts at its default on an
# update instead of «off because it was not in the list»
KNOWN_F="$STATE/components-known"
# what was installed (the shell's `update stamp`): an update repeats the same choice
INSTALLED="$STATE/install.json"

c_idx() { local i; for i in "${!C_ID[@]}"; do [[ ${C_ID[$i]} == "$1" ]] && { echo "$i"; return 0; }; done; return 1; }
c_set() { local i; i="$(c_idx "$1")" && C_ON[i]=$2; }

components_init() {
  # core files view play shot login games fork look
  C_ON=(1 1 1 1 1 "$LOGIN_ON" "$GAMES_ON" "$FORK_ON" 1)
  COMP=""
  local known i f id
  if ((UPDATE || REPAIR)); then
    [[ -f $INSTALLED ]] && COMP="$(json_field "$INSTALLED" components 2>/dev/null)"
    if [[ -n $COMP ]]; then
      # files, view, play came in 2.7: before that they went with core
      known="$(cat "$KNOWN_F" 2>/dev/null || echo "core apps look login fork")"
      for i in "${!C_ID[@]}"; do
        if [[ " $COMP " == *" ${C_ID[$i]} "* ]]; then C_ON[i]=1
        elif [[ " $known " == *" ${C_ID[$i]} "* ]]; then C_ON[i]=0; fi
      done
    elif [[ ! -x $HOME/.local/lib/yami-niri/current/niri ]]; then
      # no record of the choice (a home-only install of an old version): the fork, a 5–25 minute
      # build and ~4 GB, only for those who already run it
      c_set fork 0
    fi
  fi
  # the first install is the base alone: the rest comes from the first-run setup (--full: everything now)
  if ((SHELL_ONLY)); then
    for i in "${!C_ID[@]}"; do [[ ${C_ID[$i]} == core ]] || C_ON[i]=0; done
  fi
  # the first-run setup sends the whole choice: what is not listed goes. Games are not on its list
  # unless Steam is here: they keep their default
  if ((APPLY)); then
    for i in "${!C_ID[@]}"; do
      [[ ${C_ID[$i]} == core || ${C_ID[$i]} == look ]] && continue
      [[ ${C_ID[$i]} == games && " $APPLY_LIST " != *" games "* ]] && continue
      if [[ " $APPLY_LIST " == *" ${C_ID[$i]} "* ]]; then C_ON[i]=1; else C_ON[i]=0; fi
    done
  fi
  for f in "${COMP_FLAGS[@]}"; do
    id="${f%=*}"
    # an old script may still pass them
    [[ $C_RETIRED == *" $id "* ]] && continue
    if [[ $id == look ]] || ! c_idx "$id" >/dev/null; then
      if [[ ${f#*=} == 0 ]]; then echo "unknown option: --no-$id (see --help)" >&2; else echo "unknown option: --$id (see --help)" >&2; fi
      exit 2
    fi
    c_set "$id" "${f#*=}"
  done
  return 0
}
c_name() {
  case $1 in
    core) t "niri и сеанс" "niri and session" "niri e sessão" ;;
    files) t "файлы (yami-files)" "files (yami-files)" "arquivos (yami-files)" ;;
    view) t "картинки (yami-view)" "images (yami-view)" "imagens (yami-view)" ;;
    play) t "плеер (yami-play)" "player (yami-play)" "player (yami-play)" ;;
    shot) t "скриншоты (yami-shot)" "screenshots (yami-shot)" "capturas (yami-shot)" ;;
    login) t "экран входа" "login screen" "tela de login" ;;
    games) t "игры" "games" "jogos" ;;
    fork) t "yami-niri (форк)" "yami-niri (fork)" "yami-niri (fork)" ;;
    look) t "тема Qt и GTK" "Qt and GTK theme" "tema Qt e GTK" ;;
  esac
}
c_desc() {
  case $1 in
    core) t "niri, порталы, polkit, pipewire, сеть, bluetooth, буфер, терминал, btop, ночной свет — шеллу без них не жить" "niri, portals, polkit, pipewire, network, bluetooth, clipboard, a terminal, btop, the night light: the shell needs them" "niri, portais, polkit, pipewire, rede, bluetooth, área de transferência, um terminal, btop, luz noturna: o shell precisa deles" ;;
    files) t "файловый менеджер, диалог выбора файлов во всех программах, «показать в папке»; миниатюры, архивы, флешки, телефоны, сетевые папки" "the file manager, the file dialog of every app, «show in folder»; thumbnails, archives, drives, phones, network shares" "o gerenciador de arquivos, o diálogo de arquivos de todos os aplicativos, «mostrar na pasta»; miniaturas, compactados, pendrives, telefones, pastas de rede" ;;
    view) t "открывает картинки по умолчанию; быстрые правки: поворот, обрезка" "opens pictures by default; quick edits: rotate, crop" "abre imagens por padrão; edições rápidas: girar, recortar" ;;
    play) t "открывает видео и музыку по умолчанию (libmpv)" "opens videos and music by default (libmpv)" "abre vídeos e músicas por padrão (libmpv)" ;;
    shot) t "заморозить экран, разметить, скопировать; текст и QR (tesseract, zbar)" "freeze the screen, mark up, copy; text and QR (tesseract, zbar)" "congelar a tela, anotar, copiar; texto e QR (tesseract, zbar)" ;;
    login) if [[ -n $OTHER_DM ]]; then t "свой, как экран блокировки: greetd + yami-greet, без Qt; заменит $OTHER_DM" "our own, like the lock screen: greetd + yami-greet, no Qt; replaces $OTHER_DM" "a nossa, como a tela de bloqueio: greetd + yami-greet, sem Qt; substitui $OTHER_DM"
      else t "свой, как экран блокировки: greetd + yami-greet, без Qt, вход в niri" "our own, like the lock screen: greetd + yami-greet, no Qt, logs into niri" "a nossa, como a tela de bloqueio: greetd + yami-greet, sem Qt, entra no niri"; fi ;;
    games) t "GameMode и MangoHud в палитре" "GameMode and MangoHud in the palette" "GameMode e MangoHud na paleta" ;;
    fork)
      if ((LIGHT)); then t "наш niri из исходников: здесь ($MEM_GB ГБ, $THREADS потоков) сборка идёт около часа, поэтому выключен; пакетный niri — тот же стол" "our niri from source: here ($MEM_GB GB, $THREADS threads) the build takes about an hour, so it is off; the packaged niri is the same desktop" "nosso niri compilado: aqui ($MEM_GB GB, $THREADS threads) a compilação leva cerca de uma hora, por isso está desativada; o niri do pacote oferece o mesmo desktop"
      else t "наш niri из исходников, 5–25 мин: под этот процессор, tearing и realtime для игр; пакетный niri — запасной" "our niri from source, 5–25 min: built for this CPU, tearing and realtime for games; packaged niri stays as a fallback" "nosso niri compilado, 5–25 min: otimizado para esta CPU, tearing e tempo real nos jogos; o niri do pacote fica como alternativa"; fi ;;
    look) t "qt6ct, adw-gtk3" "qt6ct, adw-gtk3" "qt6ct, adw-gtk3" ;;
  esac
}
# look is not offered: it follows what is restyled
c_shown() {
  case $1 in
    look) return 1 ;;
    *) return 0 ;;
  esac
}
# packages or files outside the home folder; the rest installs into it (missing packages of those go
# to packages-needed)
needs_root() { case $1 in files | view | play | shot | fork | look) return 1 ;; *) return 0 ;; esac; }
# the packages of a component are optional: without sudo they go to packages-needed
can() { ! needs_root "$1" || ((SUDO)); }
on() { local i; i="$(c_idx "$1")" || return 1; ((C_ON[i])); }
off() { c_set "$1" 0; }
wants() { local i; i="$(c_idx "$1")" || return 1; ((C_WANT[i])); }
# what was chosen is kept (C_WANT: an update without sudo still knows which packages the chosen
# components want, do_need_pkgs); what this run cannot do without root is switched off
components_limit() {
  local i
  C_WANT=("${C_ON[@]}")
  for i in "${!C_ID[@]}"; do can "${C_ID[$i]}" || C_ON[i]=0; done
  # yami-niri needs clang and the compositor's libraries: without root they must already be there
  ((SUDO)) || command -v clang >/dev/null || off fork
  return 0
}
