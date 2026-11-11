# Сборка пакетов

[English](PACKAGING.md) · Русский

Заметки для пользователей Nix и сборщиков пакетов дистрибутивов: готовые бинарники и пакеты из них, flake, где yami ищет системные файлы и как отдать обновления пакетному менеджеру.

## Готовые бинарники

В каждом выпуске есть `yami-X.Y.Z-x86_64-linux.tar.gz`: шелл и модули, собранные под старую glibc, поэтому они запускаются на любой системе x86_64 с glibc 2.34 или новее, какой бы дистрибутив ни стоял. Рядом лежат `.deb` на каждую программу и `SHA256SUMS`, подписанный ключом выпуска (`SHA256SUMS.sig`, пространство имён `yami-release`, ключи в `keys/allowed_signers`). Установщик берёт их сам; делает их `tools/dist.sh` (без контейнера — под пакеты Debian 12, распакованные в `~/.cache/yami/sysroot`; в контейнере Debian 12 — с `--no-sysroot`).

Внутри архива `stage.sh` раскладывает программы туда, где их ждёт пакет:

```sh
./stage.sh --prefix /usr --destdir "$pkgdir" --managed --only yami-shell
```

`--only` берёт одну программу (`yami-shell`, с ним его службы yami-agents, `yami-files` `yami-view` `yami-play` `yami-shot` `yami-greet`), без него — все; `--managed` оставляет метку `share/yami/package-managed`, которая выключает обновления в этих же бинарниках. Службы `org.freedesktop.FileManager1` в пакете нет: в большинстве систем ею владеет Nautilus, а «показать в папке» для всех программ — личный выбор каждого (`yami-files install` спросит и положит её в домашнюю папку).

| Система | Файл |
|---|---|
| Arch | `packaging/arch/PKGBUILD`, по пакету на программу |
| Debian, Ubuntu | файлы `.deb` из выпуска, `tools/dist.sh --deb` |
| Fedora, openSUSE | `packaging/fedora/yami.spec` |
| Gentoo | `packaging/gentoo/yami-bin-*.ebuild`, флаг USE на модуль |

В Ubuntu 26.04 нет пакета niri: `install.sh` соберёт его сам, а для `.deb` niri нужен откуда-то ещё. Пакет ставит ещё `/etc/xdg/autostart/yami-shell.desktop`. Он запускает шелл (`yami-shell autostart`) при первом входе, когда конфиг niri его ещё не запускает; первая настройка открывается сама, и её шаг niri подключает yami одной строкой include. После этого шелл запускает конфиг, а запись автозапуска ничего не делает. Без systemd niri записи автозапуска не выполняет: то же сделает `yami niri connect`, один раз из терминала.

## Nix

Во flake есть пакет (шелл с его службами yami-agents, yami-files, yami-view, yami-play, yami-shot и экран входа yami-greet; модули — входы flake в версиях из `modules.lock`), модуль NixOS и модуль Home Manager. Модуль NixOS — системная половина `install.sh`: niri, службы, с которыми говорит шелл, порталы с диалогом файлов и выбором показа экрана от yami, шрифты, команды, которые запускает шелл, и экран входа. Модуль Home Manager — домашняя половина: конфиг niri, настройки шелла, kitty, fontconfig и программы по умолчанию.

```nix
{
  inputs.yami.url = "github:Trendorin/yami";
  inputs.yami.inputs.nixpkgs.follows = "nixpkgs";

  # NixOS
  imports = [ yami.nixosModules.default ];
  programs.yami.enable = true;
  programs.yami.loginScreen.user = "alice"; # чей шелл задаёт вид экрана входа

  # Home Manager
  imports = [ yami.homeManagerModules.default ];
  programs.yami.enable = true;
  programs.yami.settings = { palette = "sakura"; lang = "en"; };
}
```

Параметры NixOS:

| Параметр | По умолчанию | Что |
|---|---|---|
| `programs.yami.enable` | `false` | niri, NetworkManager, BlueZ, udisks, gvfs, polkit, профили питания, связка ключей, порталы, шрифты и команды шелла |
| `programs.yami.loginScreen.enable` | `true` | greetd запускает yami-greet внутри niri; выключенный оставляет другой менеджер входа |
| `programs.yami.loginScreen.user` | `null` | пользователь, чей шелл пишет вид экрана входа (палитра, обои, язык, экраны, раскладки) |
| `programs.yami.apps` | `true` | kitty, fastfetch, pavucontrol, btop, миниатюры видео, bsdtar |
| `programs.yami.tools` | `true` | ночной свет, текст и QR-коды со скриншотов |
| `programs.yami.appTheme` | `true` | qt6ct и adw-gtk3 для программ Qt и GTK в палитре |

Параметры Home Manager: `settings` (ключи `~/.config/yami/shell.json`, сливаются при каждой активации: эти главнее, остальное из панели настроек остаётся), `niriConfig` (подключает yami к `config.kdl` niri одной строкой include, `yami niri connect`; конфиг-ссылку в Nix store не трогает, в него строку `include optional=true "~/.config/yami/niri/yami.kdl"` добавляешь сам), `kitty`, `defaultApplications`, `extraPackages`.

Шелл сам пишет свои файлы во время работы (настройки, клавиши, экраны, порталы), поэтому модули кладут их один раз и оставляют доступными для записи. yami-niri, исправленный niri в своём репозитории, пакетом не собирается: пакетный niri — тот же стол без разрывов в играх, главного потока реального времени и правил слоёв для шторки трансляций. `nix flake check` собирает пакет и вычисляет модуль NixOS; CI запускает его при каждом пуше. Модули начинались с работы TheCodedKid в #1.

## Системные файлы

Шелл находит помощника polkit и бэкенд MTP из gvfs без жёстких путей FHS. На NixOS помощник polkit — setuid-обёртка `/run/wrappers/bin/polkit-agent-helper-1`, она идёт первой: копия помощника из `PATH` без бита setuid пароль не проверит. gvfs находится через `XDG_DATA_DIRS`. Старые пути в `/usr` остаются запасными.

Экран входа: greetd запускает `libexec/yami/greet-session` от своего пользователя greeter (установщик кладёт его в `/usr/local/lib/yami`). Он находит `bin/yami-greet` от своей папки (`YAMI_GREET` задаёт другой), niri — в `PATH`, сеансы — в папках `wayland-sessions` из `XDG_DATA_DIRS`, а yami-greet находит свои шрифты в `share/yami-greet/fonts` рядом со своим бинарником (`YAMI_GREET_FONTS`). Нужны три папки: `/var/lib/yami-greet/look` пользователя, чей шелл выкладывает вид, `/var/lib/yami-greet/state` (0700) и `/var/lib/yami-greet/log` пользователя greeter.

## Обновления через пакетный менеджер

Готовые бинарники: `--managed` у `stage.sh` (см. выше). Из исходников: соберите с признаком Cargo `package-managed` (flake так и делает):

```sh
cargo build --release --locked --workspace --features yami-shell/package-managed --manifest-path shell/Cargo.toml
```

Такая сборка не проверяет обновления по расписанию, вкладка обновлений показывает заметку вместо кнопок, а `yami update` (с любой подкомандой) пишет «Обновлениями управляет пакетный менеджер.» и выходит с 1. Обновление, переход на выпуск и откат отказываются раньше, чем тронут клон исходников или установленные бинари. Обновляйте и откатывайте пакетным менеджером. Сборка без признака сохраняет обновления.

Страница модулей в первой настройке тогда только показывает, что стоит. Не запускайте `install.sh` поверх установки, которой владеет пакетный менеджер.
