# What yami restyles in other programs: GTK, Qt and KDE, the cursor, kitty, fastfetch, btop, Firefox,
# MangoHud, foot, cava, starship. The choice lives in the shell's settings (themeApps, cursor), so an
# update, a palette change and theme.py all keep to it; settings → color → apps changes it later. Off
# means yami leaves that program's files alone (kitty: the include line it added goes). Flags:
# --restyle LIST|all|none, --shell-only. The Qt/GTK theme packages (component look) follow GTK or Qt.
# shellcheck shell=bash

T_ID=(gtk qt cursor kitty fastfetch btop firefox mangohud foot cava starship)
# up to 2.6 the key was appThemes: foot, cava and starship on top of the rest, always restyled
T_OPTIN_OLD="foot cava starship"

t_idx() { local i; for i in "${!T_ID[@]}"; do [[ ${T_ID[$i]} == "$1" ]] && { echo "$i"; return 0; }; done; return 1; }
t_on() { local i; i="$(t_idx "$1")" || return 1; ((T_ON[i])); }
t_all() { local i; for i in "${!T_ID[@]}"; do T_ON[i]=$1; done; }
# themeApps for the settings: the chosen ids without cursor (that one is the `cursor` key)
t_list() { local i o=(); for i in "${!T_ID[@]}"; do ((T_ON[i])) && [[ ${T_ID[$i]} != cursor ]] && o+=("${T_ID[$i]}"); done; printf '%s' "${o[*]}"; }
t_any() { local i; for i; do t_on "$i" && return 0; done; return 1; }

restyle_init() {
  t_all 1
  local cur id
  if [[ -f $SET_JSON ]]; then
    # a second run or an update: what the settings already say
    cur="$(restyle_saved)"
    if [[ -n $cur ]]; then
      t_all 0
      for id in $cur; do t_idx "$id" >/dev/null && T_ON[$(t_idx "$id")]=1; done
    fi
  fi
  ((SHELL_ONLY)) && t_all 0
  case ${RESTYLE:-} in
    "") ;;
    all) t_all 1 ;;
    none) t_all 0 ;;
    *)
      t_all 0
      for id in ${RESTYLE//,/ }; do
        t_idx "$id" >/dev/null || { echo "--restyle: all, none or a list of: ${T_ID[*]}" >&2; exit 2; }
        T_ON[$(t_idx "$id")]=1
      done ;;
  esac
  look_sync
}
# the list the settings keep: themeApps, or the older appThemes (its three on top of the rest), or
# everything; the cursor comes from its own key
restyle_saved() {
  local list="" id legacy=0
  if list="$(json_field "$SET_JSON" themeApps)"; then :
  elif list="$(json_field "$SET_JSON" appThemes)"; then legacy=1
  else list="${T_ID[*]}"; fi
  list=" ${list//$'\n'/ } "
  local out=() c
  c="$(json_field "$SET_JSON" cursor)" || c=yami
  for id in "${T_ID[@]}"; do
    if [[ $id == cursor ]]; then
      [[ $c != system ]] && out+=("$id")
    elif ((legacy)) && [[ " $T_OPTIN_OLD " != *" $id "* ]]; then
      out+=("$id")
    elif [[ $list == *" $id "* ]]; then
      out+=("$id")
    fi
  done
  ((${#out[@]})) && printf '%s' "${out[*]}" || printf -- '-'
}
# the Qt/GTK theme packages and apply-theme.sh: only when GTK or Qt is restyled
look_sync() {
  local v=0
  t_any gtk qt && v=1
  c_set look "$v"
}

t_name() {
  case $1 in
    gtk) printf 'GTK' ;;
    qt) t "Qt и KDE" "Qt and KDE" "Qt e KDE" ;;
    cursor) t "курсор" "cursor" "cursor" ;;
    firefox) t "Firefox и LibreWolf" "Firefox and LibreWolf" "Firefox e LibreWolf" ;;
    mangohud) printf 'MangoHud' ;;
    *) printf '%s' "$1" ;;
  esac
}
t_desc() {
  case $1 in
    gtk) t "adw-gtk3 в палитре, шрифт, тёмная или светлая: settings.ini, gtk.css, gsettings (прежние — в .bak-yami)" "adw-gtk3 in the palette, the font, dark or light: settings.ini, gtk.css, gsettings (the old ones kept as .bak-yami)" "adw-gtk3 na paleta, a fonte, escuro ou claro: settings.ini, gtk.css, gsettings (os antigos ficam como .bak-yami)" ;;
    qt) t "qt6ct с Fusion в палитре, kdeglobals (прежние — в .bak-yami)" "qt6ct with Fusion in the palette, kdeglobals (the old ones kept as .bak-yami)" "qt6ct com Fusion na paleta, kdeglobals (os antigos ficam como .bak-yami)" ;;
    cursor) t "пиксельный курсор в палитре: niri, GTK, X11-программы; выкл — системный" "the pixel cursor in the palette: niri, GTK, X11 apps; off: the system one" "o cursor em pixel na paleta: niri, GTK, apps X11; desligado: o do sistema" ;;
    kitty) t "цвета, шрифт и стекло: одна строка include yami.conf в конце kitty.conf, остальное твоё не трогается" "colours, font and glass: one line include yami.conf at the end of kitty.conf, the rest of yours stays" "cores, fonte e vidro: uma linha include yami.conf no fim do kitty.conf, o resto continua seu" ;;
    fastfetch) t "свой конфиг с логотипом 闇 (прежний — в .bak)" "its own config with the 闇 logo (the old one kept as .bak)" "uma configuração própria com o logo 闇 (a antiga fica como .bak)" ;;
    btop) t "тема yami в btop" "the yami theme in btop" "o tema yami no btop" ;;
    firefox) t "меню и шрифт: userChrome.css, только если он наш" "menus and font: userChrome.css, only one yami wrote" "menus e fonte: userChrome.css, só um escrito pelo yami" ;;
    mangohud) t "цвета и шрифт оверлея, только если конфиг наш" "the overlay's colours and font, only a config yami wrote" "cores e fonte do overlay, só uma configuração escrita pelo yami" ;;
    foot) t "терминал: include первой строкой foot.ini" "the terminal: an include as the first line of foot.ini" "o terminal: um include na primeira linha do foot.ini" ;;
    cava) t "градиент полос в его конфиге" "a gradient of the bars in its config" "um gradiente das barras na configuração dele" ;;
    starship) t "палитра yami в starship.toml" "the yami palette in starship.toml" "a paleta yami no starship.toml" ;;
  esac
}

# settings → ~/.config/yami/shell.json (do_dotfiles): themeApps and the cursor
restyle_json() { "$BIN/yami-shell" config restyle "$(t_list)" "$(t_on cursor && echo yami || echo system)"; }
