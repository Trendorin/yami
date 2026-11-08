# The end of a run: what failed (with the log), packages a repository lacks, what the chosen components
# still miss (one command), and in one sentence what to do next.
# shellcheck shell=bash

# an update: one line, or what failed (exit 1)
summary_update() {
  if ((${#FAILED[@]})); then
    warn "$(t "не вышло:" "failed:" "falhas:") ${FAILED[*]}"
    note "$(t "подробности:" "details:" "detalhes:") $LOG"
    exit 1
  fi
  say "$(t "обновлено" "updated" "atualizado")"
  [[ -s $STATE/packages-needed ]] && note "$("$BIN/yami-shell" update need --print 2>/dev/null)"
  exit 0
}

summary_install() {
  printf '\n  %s────────────────────────────────────────%s\n' "$F" "$R"
  if ((${#FAILED[@]})); then
    warn "$(t "не вышло:" "failed:" "falhas:") ${FAILED[*]}"
    note "$(t "подробности:" "details:" "detalhes:") $LOG"
  else
    printf '  %s%s%s %s%s %s%s\n' "$OK" "$(t "готово." "done." "concluído.")" "$R" "$D" "$(t "лог:" "log:" "registro:")" "$LOG" "$R"
  fi
  if [[ -s $MISSING_F ]]; then
    warn "$(t "эти пакеты не поставились (нет в репозиториях или конфликтуют)" "these packages did not install (not in the repositories or in conflict)" "estes pacotes não foram instalados (fora dos repositórios ou em conflito)") ($OS_NAME): $(sort -u "$MISSING_F" | tr '\n' ' ')"
    note "$(t "шелл работает и без них; поставь аналоги вручную, если нужно" "the shell works without them; install equivalents by hand if needed" "o shell funciona sem eles; instale equivalentes manualmente, se necessário")"
  fi
  if [[ -s $STATE/packages-needed ]] && ((!DRY)); then
    warn "$(t "для выбранного не хватает пакетов — одна команда (или настройки → обновления):" "the chosen components miss packages; one command (or settings → updates):" "faltam pacotes para os componentes escolhidos; um comando (ou configurações → atualizações):")"
    note "$("$BIN/yami-shell" update need --print 2>/dev/null)"
  fi
  if [[ $FAM == nixos ]]; then
    printf '\n  %s%s%s\n' "$T" "$(t "NixOS: система — через flake (docs/PACKAGING.md):" "NixOS: the system goes through the flake (docs/PACKAGING.md):" "NixOS: o sistema vai pelo flake (docs/PACKAGING.md):")" "$R"
    note "inputs.yami.url = \"github:Trendorin/yami\";"
    note "imports = [ yami.nixosModules.default ];  programs.yami.enable = true;"
  fi
  niri_ok || ((DRY)) || on fork || warn "$(t "нужен niri $NIRI_MIN или новее — поставь его или запусти без --no-fork" "niri $NIRI_MIN or newer is required: install it or rerun without --no-fork" "niri $NIRI_MIN ou superior é necessário: instale ou execute novamente sem --no-fork")"

  if ((${#FAILED[@]})) && ((BUILD_FAILED)); then
    printf '\n  %s%s%s\n' "$T" "$(t "Запусти установщик снова: продолжу с того же места." "Run the installer again: it continues from where it stopped." "Rode o instalador novamente: ele continua de onde parou.")" "$R"
  elif ((REPAIR)); then
    printf '\n  %s%s%s\n' "$T" "$(t "Всё обновлено и на месте." "Everything is up to date and in place." "Tudo atualizado e no lugar.")" "$R"
  elif ((FULL)); then
    if ((RUNNING_NIRI)); then
      printf '
  %s%s%s
' "$T" "$(t "Всё уже работает. Нажми Mod+Space — откроется запуск программ." "Everything is live. Press Mod+Space to open the launcher." "Tudo pronto. Pressione Mod+Space para abrir o lançador.")" "$R"
    else
      printf '
  %s%s%s
' "$T" "$(t "Выйди из системы и выбери сеанс «Niri»." "Log out and pick the «Niri» session." "Saia e escolha a sessão «Niri».")" "$R"
    fi
  elif ((RUNNING_NIRI)); then
    printf '
  %s%s%s
' "$T" "$(t "Основа готова. Сейчас откроется окно настройки." "The base is ready. The setup window opens now." "A base está pronta. A janela de configuração abre agora.")" "$R"
  elif ((NO_DM)); then
    printf '\n  %s%s%s\n' "$T" "$(t "Основа готова. Экрана входа в системе нет, поэтому запусти стол из этой консоли:" "The base is ready. This system has no login screen, so start the desktop from this console:" "A base está pronta. Este sistema não tem tela de login, então inicie a área de trabalho neste console:")" "$R"
    printf '  %s%s%s\n' "$A" "niri-session" "$R"
    printf '  %s%s%s\n' "$D" "$(t "Окно настройки предложит поставить экран входа, и в следующий раз войдёшь обычным способом." "The setup window offers a login screen, and next time you log in the usual way." "A janela de configuração oferece uma tela de login, e na próxima vez você entra normalmente.")" "$R"
  else
    printf '\n  %s%s%s\n' "$T" "$(t "Основа готова. Выйди из системы и войди в сеанс «Niri»:" "The base is ready. Log out and log in to the «Niri» session:" "A base está pronta. Saia e entre na sessão «Niri»:")" "$R"
    printf '  %s%s%s\n' "$D" "$(t "там тебя встретит окно настройки и спросит, что поставить." "a setup window will meet you there and ask what to install." "uma janela de configuração vai perguntar o que instalar.")" "$R"
  fi
  printf '
  %s%s%s
' "$F" "$(t "справка: yami help · настройка ещё раз: yami setup · удалить: ./install.sh --uninstall" "help: yami help · setup again: yami setup · remove: ./install.sh --uninstall" "ajuda: yami help · configurar de novo: yami setup · remover: ./install.sh --uninstall")" "$R"
}
