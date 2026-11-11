# The choices the install starts from: palette, panels, theme, app font, fastfetch. Flags first, then
# what the settings already say, else the defaults. Asking is the first-run setup's job (the shell's own window, `yami setup`).
# shellcheck shell=bash

# ── the choices; the palettes are the shell's own ──────────────────────────
PAL_ID=(blood ash sakura amber venom ice dusk)
PAL_RU=(кровь пепел сакура янтарь яд лёд сумрак)
PAL_EN=(blood ash sakura amber venom ice dusk)
PAL_PT=(sangue cinza sakura âmbar veneno gelo crepúsculo)
PAL_HEX=(c4161c a8a29c d9577c d98e1f 5fae34 3a9fd0 8a5cf0)
# panel look: glass (panel opacity), lift (lighter graphite), inner (cards and graphs), dim
# (behind menus); percent, written to the shell settings
LOOK_ID=(matte glass light solid)
LOOK_RU=(матовое стекло светлое плотное)
LOOK_EN=(matte glass light solid)
LOOK_PT=(fosco vidro claro sólido)
LOOK_GLASS=(78 55 85 100) LOOK_LIFT=(0 20 60 0) LOOK_INNER=(100 55 80 100) LOOK_DIM=(50 35 30 60)
# theme: dark graphite (default) or light paper; the same palette, the whole rice follows
THEME_ID=(dark light)
# app font: readable sans (default) or the pixel Tiny5 everywhere; the shell stays pixel
FONT_ID=(sans pixel)
# fastfetch: mini (default) or full with the hardware
FETCH_ID=(mini full)

# index of $2 in the array named $1, or nothing
idx_of() { local -n a=$1; local i; for i in "${!a[@]}"; do [[ ${a[$i]} == "$2" ]] && { echo "$i"; return 0; }; done; return 1; }

# a number of the settings in percent, the default when it is not one
pct_of() { # key default
  local v
  v="$(json_field "$SET_JSON" "$1")" || v=$2
  [[ $v =~ ^-?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?$ ]] || v=$2
  awk -v v="$v" 'BEGIN { printf "%d", v * 100 + 0.5 }'
}
# flags first, then what the settings file already says (one read), else the defaults
choices_init() {
  pal=0 look=0 theme=0 font=0 fetch=0 LOOK_SET=0 THEME_SET=0 FETCH_SET=0
  local cur=() i
  if [[ -f $SET_JSON ]]; then
    # palette, glass lift inner (percent), light, app font (an older install without fontUi had the
    # pixel font in apps), fastfetch
    local pa ui pixel=1
    pa="$(json_field "$SET_JSON" palette)" || pa=""
    if ui="$(json_field "$SET_JSON" fontUi)" && [[ -n $ui ]]; then pixel=0; fi
    cur=("${pa:--}" "$(pct_of glass 0.78)" "$(pct_of lift 0)" "$(pct_of inner 1)" "$(json_true "$SET_JSON" light && echo 1 || echo 0)" "$pixel" "$(json_true "$SET_JSON" fetchFull && echo 1 || echo 0)")
  fi
  if [[ -n $PALETTE ]]; then
    pal="$(idx_of PAL_ID "$PALETTE")" || { echo "--palette: ${PAL_ID[*]}" >&2; exit 2; }
  elif ((${#cur[@]} == 7)); then
    pal="$(idx_of PAL_ID "${cur[0]}")" || pal=0
  fi
  if [[ -n $LOOK_OPT ]]; then
    look="$(idx_of LOOK_ID "$LOOK_OPT")" || { echo "--look: ${LOOK_ID[*]}" >&2; exit 2; }
    LOOK_SET=1
  elif ((${#cur[@]} == 7)); then
    for i in "${!LOOK_ID[@]}"; do [[ "${LOOK_GLASS[$i]} ${LOOK_LIFT[$i]} ${LOOK_INNER[$i]}" == "${cur[1]} ${cur[2]} ${cur[3]}" ]] && look=$i; done
  fi
  if [[ -n $THEME_OPT ]]; then
    theme="$(idx_of THEME_ID "$THEME_OPT")" || { echo "--theme: dark or light" >&2; exit 2; }
    THEME_SET=1
  elif ((${#cur[@]} == 7)); then
    theme=${cur[4]}
  fi
  if [[ -n $FONT_OPT ]]; then
    font="$(idx_of FONT_ID "$FONT_OPT")" || { echo "--font: sans or pixel" >&2; exit 2; }
  elif ((${#cur[@]} == 7)); then
    font=${cur[5]}
  fi
  if [[ -n $FETCH_OPT ]]; then
    fetch="$(idx_of FETCH_ID "$FETCH_OPT")" || { echo "--fetch: mini or full" >&2; exit 2; }
    FETCH_SET=1
  elif ((${#cur[@]} == 7)); then
    fetch=${cur[6]}
  fi
  return 0
}

theme_name() { case ${THEME_ID[$1]} in dark) t "тёмная" "dark" "escuro" ;; light) t "светлая" "light" "claro" ;; esac; }
theme_desc() {
  case ${THEME_ID[$1]} in
    dark) t "графит и акцент — как задумано" "graphite and the accent: the original look" "grafite e destaque: a aparência original" ;;
    light) t "светлая бумага, акцент темнее; GTK, Qt, kitty и курсор — тоже" "light paper, a deeper accent; GTK, Qt, kitty and the cursor follow" "papel claro, destaque mais escuro; GTK, Qt, kitty e o cursor acompanham" ;;
  esac
}
fetch_name() { case ${FETCH_ID[$1]} in mini) t "короткий" "mini" "resumido" ;; full) t "расширенный" "extended" "completo" ;; esac; }
fetch_desc() {
  case ${FETCH_ID[$1]} in
    mini) t "кто, система, окружение — без железа" "who, system, desktop: no hardware" "usuário, sistema e desktop: sem hardware" ;;
    full) t "ещё ядро, пакеты, процессор, видеокарта, память, диск, экран" "also kernel, packages, CPU, GPU, memory, disk, display" "também kernel, pacotes, CPU, GPU, memória, disco e tela" ;;
  esac
}
font_name() { case ${FONT_ID[$1]} in sans) t "обычный (Noto Sans)" "regular (Noto Sans)" "normal (Noto Sans)" ;; pixel) t "пиксельный (Tiny5)" "pixel (Tiny5)" "pixel (Tiny5)" ;; esac; }
font_desc() {
  case ${FONT_ID[$1]} in
    sans) t "программы читаются как обычно; шелл, файлы и экран входа — пиксельные" "apps read as usual; the shell, files and login screen stay pixel" "aplicativos com leitura normal; shell, arquivos e tela de login continuam em pixel" ;;
    pixel) t "пиксельный шрифт во всех программах, ровно по сетке экрана" "the pixel font in every app, exactly on the screen's grid" "fonte em pixel em todos os aplicativos, alinhada à grade da tela" ;;
  esac
}

look_name() { t "${LOOK_RU[$1]}" "${LOOK_EN[$1]}" "${LOOK_PT[$1]}"; }
look_desc() {
  case ${LOOK_ID[$1]} in
    matte) t "тёмный графит, панели почти сплошные — как задумано" "dark graphite, nearly solid panels: the original look" "grafite escuro, painéis quase sólidos: a aparência original" ;;
    glass) t "панели и карточки прозрачные, стол просвечивает" "translucent panels and cards, the desktop shows through" "painéis e cartões translúcidos; a área de trabalho aparece ao fundo" ;;
    light) t "графит светлее, без глухого чёрного" "lighter graphite, no deep black" "grafite mais claro, sem preto intenso" ;;
    solid) t "всё сплошное, сильное затемнение под меню" "fully solid, strong dim behind menus" "totalmente sólido, escurecimento forte atrás dos menus" ;;
  esac
}

pal_name() { t "${PAL_RU[$1]}" "${PAL_EN[$1]}" "${PAL_PT[$1]}"; }

