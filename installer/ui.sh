# What the installer looks like in the terminal. Colours from the palette, the
# message helpers (say, note, warn), the animated banner, keys for the menus and the step runner:
# a spinner and a progress bar, the log, «not needed» (exit 99) and failures with the log's tail.
# Sourced by install.sh; defines only (install.sh calls paint and ui_traps).
# shellcheck shell=bash

# t "русский" "english" "português" [VALUE…]: the text in the chosen language, each {} replaced by the next
# VALUE. Spanish, German, French, Ukrainian and Polish come from the table installer/lang/CODE.sh keyed by
# the English text (lang_load); a line missing there, or a missing Portuguese one, shows in English
declare -gA TR=()
t() {
  local s v
  case $L in
    ru) s=$1 ;;
    en) s=$2 ;;
    pt-BR) s=${3:-$2} ;;
    *) s=$2; [[ -n $2 && -n ${TR[$2]:-} ]] && s=${TR[$2]} ;;
  esac
  shift $(($# < 3 ? $# : 3))
  for v in "$@"; do s=${s/"{}"/"$v"}; done
  printf '%s' "$s"
}
# the table of the chosen language (after L is set or changed); ru, en and pt-BR live in the t calls
lang_load() {
  TR=()
  case $L in
    # shellcheck source=/dev/null
    es | de | fr | uk | pl) [[ -f $ROOT/installer/lang/$L.sh ]] && . "$ROOT/installer/lang/$L.sh" ;;
  esac
  return 0
}
# the l key on the first screen: every language in turn
lang_next() {
  case $1 in ru) echo en ;; en) echo pt-BR ;; pt-BR) echo es ;; es) echo de ;; de) echo fr ;; fr) echo uk ;; uk) echo pl ;; *) echo ru ;; esac
}
# the installer's language from a flag or a locale (ru_RU.UTF-8, pt_BR, pt, de_AT…); nothing for the rest
lang_code() {
  case "${1,,}" in
    ru | ru[-_.@]*) printf '%s' ru ;;
    en | en[-_.@]*) printf '%s' en ;;
    pt | pt[-_.@]*) printf '%s' pt-BR ;;
    es | es[-_.@]*) printf '%s' es ;;
    de | de[-_.@]*) printf '%s' de ;;
    fr | fr[-_.@]*) printf '%s' fr ;;
    uk | uk[-_.@]*) printf '%s' uk ;;
    pl | pl[-_.@]*) printf '%s' pl ;;
  esac
}

# ── paint ───────────────────────────────────────────────────────────────────
rgb() { printf '%d;%d;%d' "0x${1:0:2}" "0x${1:2:2}" "0x${1:4:2}"; }
# accent mixed towards white (k>0) or black (k<0), in percent
shade() {
  local h=$1 k=$2 c out="" v
  for c in 0 2 4; do
    v=$((0x${h:c:2}))
    if ((k > 0)); then v=$((v + (255 - v) * k / 100)); else v=$((v * (100 + k) / 100)); fi
    out+="$v;"
  done
  printf '%s' "${out%;}"
}
paint() {
  [[ -t 1 ]] || { A="" AB="" AH="" AD=""; return; }
  local h=${PAL_HEX[$pal]}
  A=$'\e'"[38;2;$(rgb "$h")m"
  AB=$'\e'"[48;2;$(rgb "$h")m"
  AH=$'\e'"[38;2;$(shade "$h" 55)m"
  AD=$'\e'"[38;2;$(shade "$h" -55)m"
}
T=$'\e[38;2;235;229;228m' D=$'\e[38;2;150;140;140m' F=$'\e[38;2;88;78;78m'
OK=$'\e[38;2;120;200;120m' WARN=$'\e[38;2;255;170;40m' BAD=$'\e[38;2;255;74;61m'
B=$'\e[1m' R=$'\e[0m'
[[ -t 1 ]] || T="" D="" F="" OK="" WARN="" BAD="" B="" R=""

pad() { printf '%s%*s' "$1" $(($2 - ${#1} > 0 ? $2 - ${#1} : 0)) ''; }
say() { printf "  %s▪%s %s%s%s\n" "$A" "$R" "$T" "$*" "$R"; }
note() { printf "    %s%s%s\n" "$F" "$*" "$R"; }
warn() { printf "  %s!%s %s%s%s\n" "$WARN" "$R" "$T" "$*" "$R"; }
nap() { ((ANIM)) && sleep "$1"; return 0; }

cleanup() { printf '\e[?25h%s' "$R"; }
ui_traps() {
  trap cleanup EXIT
  trap 'cleanup; echo; exit 130' INT
  trap 'cleanup; exit 143' TERM
  # the terminal may die mid-install (kitty reloads fonts and themes): finish the steps anyway
  trap '' HUP
}

# ── banner: the logo dissolves in on a Bayer grid, then a light sweep ───────
LOGO1="▀▄▀ ▄▀█ █▀▄▀█ █"
LOGO2=" █  █▀█ █ ▀ █ █"
BAYER=(0 8 2 10 12 4 14 6 3 11 1 9 15 7 13 5)
logo_row() { # row text frame sweep
  local s=$2 f=$3 sw=$4 out="" i ch th
  for ((i = 0; i < ${#s}; i++)); do
    ch=${s:i:1}
    th=${BAYER[($1 % 4) * 4 + i % 4]}
    if [[ $ch == " " ]]; then out+=" "
    elif ((th < f - 2)); then
      if ((sw >= 0 && (i == sw || i == sw - 1))); then out+="$T$B$ch$R"
      elif ((sw >= 0 && i == sw - 2)); then out+="$AH$ch"
      else out+="$A$ch"; fi
    elif ((th < f)); then out+="$AD░"
    else out+=" "; fi
  done
  printf '  %s%s\e[K\n' "$out" "$R"
}
SUBTITLE=""
banner() { # 1 — animate
  SUBTITLE="$(t "рабочий стол для niri" "a desktop for niri" "uma área de trabalho para niri")"
  local f sw
  printf '\n'
  if (($1)) && ((ANIM)); then
    printf '\e[?25l'
    for ((f = 0; f <= 18; f++)); do
      logo_row 0 "$LOGO1" "$f" -1
      logo_row 1 "$LOGO2" "$f" -1
      printf '\e[2A'
      sleep 0.022
    done
    for ((sw = 0; sw <= ${#LOGO1} + 3; sw += 2)); do
      logo_row 0 "$LOGO1" 99 "$sw"
      logo_row 1 "$LOGO2" 99 "$sw"
      printf '\e[2A'
      sleep 0.012
    done
  fi
  logo_row 0 "$LOGO1" 99 -1
  logo_row 1 "$LOGO2" 99 -1
  if (($1)) && ((ANIM)); then
    local line="────────────────────────────────────────" i
    printf '  %s' "$F"
    for ((i = 0; i < ${#line}; i += 4)); do printf '%s' "${line:i:4}"; sleep 0.008; done
    printf '%s\n  %s' "$R" "$D"
    for ((i = 0; i < ${#SUBTITLE}; i++)); do printf '%s' "${SUBTITLE:i:1}"; sleep 0.012; done
    printf '%s\n\n' "$R"
  else
    printf '  %s────────────────────────────────────────%s\n' "$F" "$R"
    printf '  %s%s%s\n\n' "$D" "$SUBTITLE" "$R"
  fi
}

# ── the first screen: what is about to happen, in plain words ───────────────
welcome_rows() { printf '  %s%s%s  %s%s%s\n' "$F" "$(pad "$1" 12)" "$R" "$T" "$2" "$R"; }
# free space in the home folder, GB; how much this install needs
disk_free() { df -Pk "$HOME" 2>/dev/null | awk 'NR == 2 { printf "%d", $4 / 1048576 }'; }
disk_need() {
  local gb=3
  ((PREBUILT)) && gb=1
  ((FULL)) && gb=6
  on fork && gb=$((gb + 4))
  echo "$gb"
}
# minutes on this machine, roughly: compiling is most of it
time_guess() {
  ((PREBUILT)) && ! on fork && { printf '2–5'; return; }
  local m=$((4 + 40 / (THREADS > 0 ? THREADS : 2)))
  ((FULL)) && m=$((m * 2))
  printf '%d–%d' "$m" $((m * 2))
}
# the network is needed for packages and sources; a local source tree does without
net_check() {
  [[ -n ${YAMI_MODULES_DIR:-} || $YAMI_REPO == file:* ]] && return 0
  command -v curl >/dev/null || return 0
  curl -fsS -o /dev/null --connect-timeout 5 --max-time 8 https://github.com 2>/dev/null
}
welcome() {
  local have gb net=1 tries=0
  net_check || net=0
  while :; do
    printf '\e[H\e[J'
    banner 1
    if ((REPAIR)); then
      printf '  %s%s%s\n' "$T" "$(t "yami уже стоит: обновлю и починю, твой выбор и настройки останутся." "yami is already here: I will update and repair it, your choices and settings stay." "o yami já está instalado: vou atualizar e reparar, suas escolhas e configurações ficam.")" "$R"
      printf '  %s%s%s\n\n' "$D" "$(t "Перед изменениями сохраню точку отката." "A rollback point is saved before any change." "Um ponto de restauração é salvo antes de qualquer mudança.")" "$R"
    elif ((FULL)); then
      printf '  %s%s%s\n\n' "$T" "$(t "Поставлю всё сразу: niri, шелл и все модули." "I will install everything at once: niri, the shell and all modules." "Vou instalar tudo de uma vez: niri, o shell e todos os módulos.")" "$R"
    else
      printf '  %s%s%s\n' "$T" "$(t "Сейчас поставлю только основу: niri, шелл и терминал." "I will install only the base now: niri, the shell and a terminal." "Vou instalar só a base agora: niri, o shell e um terminal.")" "$R"
      printf '  %s%s%s\n\n' "$D" "$(t "Файлы, плеер, скриншоты, экран входа и темы выберешь в первой настройке, при первом входе." "Files, player, screenshots, login screen and themes are chosen in the first-run setup at the first login." "Arquivos, player, capturas, tela de login e temas são escolhidos na configuração inicial, no primeiro login.")" "$R"
    fi
    welcome_rows "$(t "система" "system" "sistema")" "$OS_NAME · $MEM_GB $(t "ГБ" "GB" "GB") · $THREADS $(t "потоков" "threads" "núcleos")"
    if ((SUDO)); then welcome_rows "$(t "пароль" "password" "senha")" "$(t "спрошу один раз, для пакетов" "asked once, for the packages" "pedida uma vez, para os pacotes")"; fi
    welcome_rows "$(t "место" "location" "local")" "~/.local/bin, ~/.config/niri ($(t "прежнее сохраню рядом" "what was there is kept next to it" "o que existia fica ao lado"))"
    welcome_rows "$(t "время" "time" "tempo")" "$(t "примерно {} минут" "about {} minutes" "cerca de {} minutos" "$(time_guess)")"
    have="$(disk_free)" gb="$(disk_need)"
    if [[ -n $have ]] && ((have < gb)); then
      welcome_rows "$(t "диск" "disk" "disco")" "$WARN$(t "свободно {} ГБ, а нужно около {} ГБ — сборка может не влезть" "{} GB free, about {} GB are needed: the build may not fit" "{} GB livres, são necessários cerca de {} GB: a compilação pode não caber" "$have" "$gb")$R"
    else
      welcome_rows "$(t "диск" "disk" "disco")" "$(t "нужно около {} ГБ, свободно {} ГБ" "about {} GB needed, {} GB free" "cerca de {} GB necessários, {} GB livres" "$gb" "$have")"
    fi
    ((net)) || welcome_rows "$(t "сеть" "network" "rede")" "$WARN$(t "GitHub не отвечает: проверь интернет и нажми enter" "GitHub does not answer: check the internet and press enter" "o GitHub não responde: verifique a internet e pressione enter")$R"
    if ((COUNT)) && [[ ! -e $STATE/counted && -z ${DO_NOT_TRACK:-} && -z ${YAMI_NO_COUNT:-} ]]; then
      welcome_rows "$(t "счётчик" "counter" "contador")" "$(t "один анонимный запрос в конце, без данных (--no-count — без него)" "one anonymous request at the end, no data (--no-count skips it)" "uma requisição anônima no final, sem dados (--no-count desativa)")"
    fi
    ((DRY)) && welcome_rows "--dry-run" "$(t "только покажу план, ничего не изменю" "only showing the plan, nothing changes" "só mostro o plano, nada muda")"
    ((RUNNING_NIRI)) && welcome_rows "niri" "$(t "уже запущен: шелл появится сразу" "already running: the shell appears right away" "já em execução: o shell aparece na hora")"
    printf '\n  %s%s%s  %s  %s%s%s  %s  %s%s%s  %s\n' "$A" "enter" "$R" "$(t "поставить" "install" "instalar")" "$A" "l" "$R" "$(t "язык" "language" "idioma")" "$A" "q" "$R" "$(t "выйти" "quit" "sair")"
    case "$(key)" in
      enter | y)
        # GitHub still silent: one more Enter goes on anyway (a proxy may let git through and not curl)
        if ((!net)) && ! net_check && ((tries++ == 0)); then continue; fi
        printf '\n'
        return 0 ;;
      l | "д") L="$(lang_next "$L")"; lang_load ;;
      q | "й" | esc | eof) bye ;;
    esac
  done
}

# ── keys ────────────────────────────────────────────────────────────────────
key() {
  local k rest more
  # терминал закрыт (ssh оборвался, TTY ушёл): не крутиться в меню, а выйти
  IFS= read -rsn1 k || { echo eof; return; }
  if [[ $k == $'\e' ]]; then
    # по ssh и на медленном TTY хвост последовательности приходит позже: ждём дольше и
    # дочитываем всё (Home/End/F-клавиши длиннее двух байт), чтобы остаток не стал «клавишами»
    read -rsn2 -t 0.15 rest
    while IFS= read -rsn1 -t 0.01 more; do rest+=$more; done
    case "$rest" in "[A" | "OA") echo up ;; "[B" | "OB") echo down ;; "[C" | "OC") echo right ;; "[D" | "OD") echo left ;; "") echo esc ;; *) echo other ;; esac
  elif [[ $k == "" ]]; then echo enter
  elif [[ $k == " " ]]; then echo space
  else echo "$k"; fi
}
bye() { printf '\e[H\e[J  %s\n' "$(t "ну и ладно. ничего не тронуто." "ok, nothing was touched." "certo, nada foi alterado.")"; exit 0; }
screen() { printf '\e[H\e[J'; banner 0; }

# ── steps: spinner, what is happening right now, a bar that moves with the work ─────────
STEPS_T=() STEPS_F=() STEPS_W=()
# how long a step usually takes, in parts: the bar follows the work, not the number of steps
step_weight() {
  case $1 in
    do_build_shell) echo 40 ;;
    do_prebuilt_bins) echo 2 ;;
    do_build_module) echo 10 ;;
    do_build_greet) echo 8 ;;
    do_niri_source) echo 60 ;;
    do_xwls_source) echo 8 ;;
    pkgs) echo 24 ;;
    do_fetch_modules | pm_refresh) echo 4 ;;
    *) echo 1 ;;
  esac
}
add() { local title=$1; shift; STEPS_T+=("$title"); STEPS_F+=("$(printf '%q ' "$@")"); STEPS_W+=("$(step_weight "$1")"); }
FAILED=() SKIPPED=() BUILD_FAILED=0 FATAL_REASON=""
SPIN=(⣾ ⣽ ⣻ ⢿ ⡿ ⣟ ⣯ ⣷)
W_TOTAL=1 W_DONE=0
cols() { local c; c="$(tput cols 2>/dev/null)"; echo "${c:-100}"; }
# 0–100: the finished steps plus the share of the current one
percent() { echo $(((W_DONE * 100 + $1 * ${STEPS_W[$2]}) / W_TOTAL)); }
progress() { # percent frame
  local w=32 fill=$(($1 * 32 / 100)) s="" j hl=$(($2 % 44 - 6))
  for ((j = 0; j < w; j++)); do
    if ((j < fill)); then
      if ((j == hl || j == hl + 1)); then s+="$T█"; elif ((j == hl - 1 || j == hl + 2)); then s+="$AH█"; else s+="$A█"; fi
    elif ((j == fill)); then s+="$AD▓"
    else s+="$F░"; fi
  done
  printf '  %s%s %s%d%%%s' "$s" "$R" "$D" "$1" "$R"
}
elapsed() { local s=$1; ((s >= 60)) && printf '%dm%02ds' $((s / 60)) $((s % 60)) || printf '%ds' "$s"; }
# how many crates this build compiles, roughly: the lockfile of the crate being built
crates_of() {
  local f
  case $1 in
    do_build_shell) f="$ROOT/shell/Cargo.lock" ;;
    do_build_module) f="$(m_src "yami-$2" 2>/dev/null)/Cargo.lock" ;;
    do_build_greet) f="$(m_src yami-greet 2>/dev/null)/Cargo.lock" ;;
    *) return 0 ;;
  esac
  grep -c '^name = ' "$f" 2>/dev/null
}
# the last line of the log, plain and cut to the terminal
now_doing() {
  local l room=$(($1 - 8))
  l="$(tail -n 3 "$LOG" 2>/dev/null | grep -v '^### ' | tail -n 1 | sed -e "s/$(printf '\033')\[[0-9;]*[A-Za-z]//g" -e 's/^[[:space:]]*//')"
  ((${#l} > room)) && l="${l:0:room-1}…"
  printf '%s' "$l"
}
# one sentence on why a step failed, from the end of the log; empty when there is nothing to say
explain() {
  local tl
  tl="$(tail -n 60 "$LOG" 2>/dev/null)"
  case "$tl" in
    *"signature of the ready binaries"* | *"carry no signature"* | *"match their checksum"*) t "готовые бинари не прошли проверку подписи: ничего не поставлено; собрать из исходников — ./install.sh --from-source" "the ready binaries failed the signature check: nothing was installed; to build from source: ./install.sh --from-source" "os binários prontos falharam na verificação da assinatura: nada foi instalado; para compilar: ./install.sh --from-source" ;;
    *"could not download"*) t "готовые бинари не скачались: проверь интернет или собери из исходников (./install.sh --from-source)" "the ready binaries could not be downloaded: check the internet or build from source (./install.sh --from-source)" "os binários prontos não puderam ser baixados: verifique a internet ou compile (./install.sh --from-source)" ;;
    *"No space left on device"*) t "кончилось место на диске: освободи несколько гигабайт и запусти снова" "the disk is full: free a few gigabytes and run again" "o disco está cheio: libere alguns gigabytes e rode novamente" ;;
    *"no such version in its repository"*) t "у модуля нет нужной версии: похоже, выпуск ещё публикуется; подожди и запусти снова" "a module does not have the needed version: the release may still be going out; wait and run again" "falta a versão necessária de um módulo: a publicação pode estar em andamento; aguarde e rode novamente" ;;
    *"signature does not check out"*) t "подпись модуля не сошлась, ничего не поставлено: запусти установку заново; если повторится — напиши автору" "a module's signature did not match, nothing was installed: run again; if it repeats, tell the author" "a assinatura de um módulo não confere, nada foi instalado: rode novamente; se repetir, avise o autor" ;;
    *"unable to lock database"* | *"Could not get lock"* | *"could not get lock"* | *"is locked by another"*) t "сейчас работает другой менеджер пакетов: дождись его и запусти снова" "another package manager is running: wait for it and run again" "outro gerenciador de pacotes está em execução: aguarde e rode novamente" ;;
    *"failed retrieving file"* | *"404  Not Found"* | *"Failed to fetch"* | *"Status code: 404"*) t "база пакетов устарела: обнови систему и запусти снова (Arch: sudo pacman -Syu)" "the package index is out of date: update the system and run again (Arch: sudo pacman -Syu)" "o índice de pacotes está desatualizado: atualize o sistema e rode novamente (Arch: sudo pacman -Syu)" ;;
    *"Could not resolve host"* | *"Temporary failure in name resolution"* | *"Failed to connect"* | *"Network is unreachable"* | *"Connection timed out"* | *"unable to access"*) t "нет интернета или GitHub недоступен: проверь сеть и запусти снова" "no internet or GitHub is unreachable: check the network and run again" "sem internet ou GitHub indisponível: verifique a rede e rode novamente" ;;
    *"signal: 9"* | *"SIGKILL"* | *"Cannot allocate memory"* | *"out of memory"*) t "не хватило памяти на сборку: закрой тяжёлые программы и запусти снова (CARGO_BUILD_JOBS=1 — медленнее, но надёжнее)" "not enough memory for the build: close heavy apps and run again (CARGO_BUILD_JOBS=1 is slower but safer)" "memória insuficiente para compilar: feche programas pesados e rode novamente (CARGO_BUILD_JOBS=1 é mais lento, porém mais seguro)" ;;
    *"ssh-keygen"*) t "нет ssh-keygen, подпись не проверить: поставь openssh и запусти снова" "ssh-keygen is missing, signatures cannot be checked: install openssh and run again" "ssh-keygen ausente, assinaturas não podem ser verificadas: instale o openssh e rode novamente" ;;
    *"linker \`cc\` not found"*) t "нет компилятора C: поставь gcc и запусти снова" "there is no C compiler: install gcc and run again" "sem compilador C: instale o gcc e rode novamente" ;;
    *"was not found in the pkg-config search path"*) t "не хватает библиотеки для сборки: запусти установщик без --no-sudo, он поставит её сам" "a library for the build is missing: run the installer without --no-sudo, it installs it" "falta uma biblioteca para compilar: rode o instalador sem --no-sudo, ele a instala" ;;
    *"requires rustc"* | *"is not supported by the following"*) t "Rust слишком старый: rustup update, затем запусти снова" "Rust is too old: rustup update, then run again" "Rust antigo demais: rustup update e rode novamente" ;;
  esac
}
# a line for whoever reads our output (the first-run setup, settings → updates): [percent/100] title
step_line() { printf '  %s▸%s %s[%d/100] %s%s\n' "$A" "$R" "$T" "$1" "$2" "$R"; }
# foreground output: every line as it is, and the percent moves while something compiles
step_pipe() { # index crates
  local line n=0 i=$1 total=${2:-0} last=-1 share p
  while IFS= read -r line; do
    printf '      %s\n' "$line"
    if [[ $line == *Compiling\ * ]] && ((total > 0)); then
      n=$((n + 1))
      share=$((n * 100 / total))
      ((share > 97)) && share=97
      p="$(percent "$share" "$i")"
      if ((p != last)); then
        last=$p
        step_line "$p" "${STEPS_T[$i]}"
      fi
    fi
  done
}
step() { # index
  local i=$1 title=${STEPS_T[$1]} t0=$SECONDS rc f=0
  printf '\n### %s\n' "$title" >>"$LOG"
  if ((DRY)); then
    local cmd=${STEPS_F[$i]} room=$(($(cols) - ${#title} - 8))
    ((${#cmd} > room)) && cmd="${cmd:0:room-1}…"
    printf '  %s·%s %s%s%s %s%s%s\n' "$A" "$R" "$T" "$title" "$R" "$F" "$cmd" "$R"
    return
  fi
  local crates
  # shellcheck disable=SC2086 # the function and its arguments
  crates="$(crates_of ${STEPS_F[$i]})"
  if [[ ! -t 1 || $SU == sudo || $SU == doas || $SU == "sudo -A" ]]; then
    # no terminal, or root without the shared root shell (its prompts must be seen): run in the
    # foreground; [percent/100]: the first-run setup and the shell's updates tab draw their progress from it
    step_line "$(percent 0 "$i")" "$title"
    # the step writes to the log and the reader follows the log until the step ends. Not a pipe: a
    # service the step starts would inherit it and everyone downstream would wait for that service
    local from reader pidf="$CACHE/step-tail.pid"
    from=$(($(wc -c <"$LOG") + 1))
    (eval "${STEPS_F[$i]}") >>"$LOG" 2>&1 <&0 &
    local pid=$!
    { tail -c +"$from" -f "$LOG" 2>/dev/null & echo $! >"$pidf"; wait; } | step_pipe "$i" "$crates" &
    reader=$!
    wait "$pid"
    rc=$?
    # the reader prints what is left, then it is told to stop
    sleep 0.2
    kill "$(cat "$pidf" 2>/dev/null)" 2>/dev/null
    wait "$reader" 2>/dev/null
    rm -f -- "$pidf"
  else
    # root commands go to the root shell authorised before the steps (root_start): nothing here
    # asks for a password, so every step can run behind the spinner
    (eval "${STEPS_F[$i]}") >>"$LOG" 2>&1 </dev/null &
    local pid=$! width share=0 doing="" c0 c n p tm
    width="$(cols)"
    c0="$(grep -c 'Compiling ' "$LOG" 2>/dev/null)"
    printf '\e[?25l'
    while kill -0 "$pid" 2>/dev/null; do
      if ((f % 6 == 0)); then
        doing="$(now_doing "$width")"
        if ((${crates:-0} > 0)); then
          c="$(grep -c 'Compiling ' "$LOG" 2>/dev/null)"
          n=$((c - c0))
          share=$((n * 100 / crates))
          ((share > 97)) && share=97
        fi
      fi
      p="$(percent "$share" "$i")"
      # how long is left, once the bar has moved enough to say
      tm=""
      if ((p >= 8 && SECONDS - RUN_T0 > 20)); then
        tm=" ~$(elapsed $(((SECONDS - RUN_T0) * (100 - p) / p)))"
      fi
      printf '\r\e[K  %s%s%s %s%s%s %s%s%s\n\e[K    %s%s%s\n\e[K' "$A" "${SPIN[f % 8]}" "$R" "$T" "$title" "$R" "$F" "$(elapsed $((SECONDS - t0)))" "$R" "$F" "$doing" "$R"
      progress "$p" "$f"
      printf '%s%s%s\e[2A\r' "$F" "$tm" "$R"
      ((f++))
      sleep 0.07
    done
    wait "$pid"
    rc=$?
    printf '\r\e[K\n\e[K\n\e[K\e[2A\r'
  fi
  local dt
  dt="$(elapsed $((SECONDS - t0)))"
  if ((rc == 0)); then
    printf '\r\e[K  %s✓%s %s%s%s %s%s%s\n\e[K' "$OK" "$R" "$T" "$title" "$R" "$F" "$dt" "$R"
  elif ((rc == 99)); then
    printf '\r\e[K  %s–%s %s%s%s %s%s%s\n\e[K' "$F" "$R" "$D" "$title" "$R" "$F" "$(t "не нужно" "not needed" "desnecessário")" "$R"
    SKIPPED+=("$title")
  else
    printf '\r\e[K  %s✗%s %s%s%s %s— %s%s\n\e[K' "$BAD" "$R" "$T" "$title" "$R" "$F" "$(t "хвост лога:" "log tail:" "final do registro:")" "$R"
    tail -n 6 "$LOG" | sed "s/^/      $F/; s/\$/$R/"
    local why
    why="$(explain)"
    [[ -n $why ]] && warn "$why"
    FAILED+=("$title")
  fi
  printf '\e[?25h'
}
# a failure of one of these stops the run; steps before the swap changed nothing
step_fatal() { case $1 in do_fetch_modules | do_build_* | do_prebuilt_bins | do_install_bins) return 0 ;; pkgs) [[ -e $CACHE/packages-stale ]] ;; *) return 1 ;; esac; }
# these change what the session starts from: a failure here puts the snapshot back
step_critical() { case $1 in do_install_bins | do_niri_config | do_refresh) return 0 ;; *) return 1 ;; esac; }
run_steps() {
  local i nf w cmd
  W_TOTAL=0
  for w in "${STEPS_W[@]}"; do W_TOTAL=$((W_TOTAL + w)); done
  ((W_TOTAL)) || W_TOTAL=1
  RUN_T0=$SECONDS
  for ((i = 0; i < ${#STEPS_T[@]}; i++)); do
    nf=${#FAILED[@]}
    cmd=${STEPS_F[$i]%% *}
    # every root step comes before the build: the root shell is closed here, so no build script of
    # a crate (minutes of code from crates.io) runs while its pipes are open (see root_start). A step
    # after this point that still asked for root would fail (asroot: «the root shell is gone»)
    [[ $cmd == do_build_* || $cmd == do_prebuilt_bins ]] && root_stop
    step "$i"
    W_DONE=$((W_DONE + ${STEPS_W[$i]}))
    ((${#FAILED[@]} > nf)) || continue
    # first the steps that change what the session starts from: with a rollback point the run puts it
    # back (do_install_bins is fatal too, but a swap that stopped half way has already changed something)
    if step_critical "$cmd" && ((!DRY)) && declare -F snapshot_restore >/dev/null && [[ -s $CACHE/snapshot-now ]]; then
      if snapshot_restore; then
        warn "$(t "вернул всё как было до этого запуска: программы и конфиги" "everything is back as it was before this run: programs and configs" "tudo voltou como estava antes desta execução: programas e configurações")"
      fi
      BUILD_FAILED=1
      break
    fi
    if step_fatal "$cmd"; then
      # the binaries are swapped in only after every build has passed: nothing has changed yet
      BUILD_FAILED=1
      if ((UPDATE)); then
        warn "$(t "сборка не удалась — обновление остановлено, стоит прежняя версия" "the build failed: update stopped, the previous version stays" "a compilação falhou: atualização interrompida, a versão anterior é mantida")"
      else
        warn "$(t "не вышло — ничего не изменилось; поправь причину и запусти установку снова" "it failed and nothing changed; fix the cause and run the install again" "falhou e nada mudou; corrija a causa e rode a instalação novamente")"
      fi
      break
    fi
  done
  if [[ -t 1 ]] && ((!DRY)); then
    local f
    for ((f = 0; f < 22; f++)); do printf '\r'; progress 100 $((f * 2)); nap 0.02; done
    printf '\n'
  fi
}

backup() { [[ -e "$1" && ! -L "$1" ]] && mv -- "$1" "$1.bak-$STAMP"; return 0; }
