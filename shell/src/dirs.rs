// Где yami держит своё в домашней папке: всё в ~/.config/yami, как у других шеллов.
//   shell.json                     настройки шелла (до 3.0 — ~/.config/yami-shell.json)
//   style.json, style.<модуль>.json   вид модулей; files.json, shot.json, agents.json — их настройки
//   scripts/                       модули бара и виджеты из скриптов (до 3.0 — modules/)
//   themes/, templates/            свои темы и шаблоны перекраски
//   niri/                          часть yami в конфиге niri (niricfg.rs)
// `migrate` переносит файлы старых версий; зовётся при каждом запуске, без старых файлов — пара stat.

use std::path::{Path, PathBuf};
pub use yami_px::paths::yami_dir;

pub fn niri_dir() -> PathBuf {
    yami_dir().join("niri")
}

pub fn scripts_dir() -> PathBuf {
    yami_dir().join("scripts")
}

/// файлы niri до 3.0 (рядом с config.kdl) → их имена в ~/.config/yami/niri
pub const OLD_NIRI: [(&str, &str); 5] = [("yami-binds.kdl", "binds.kdl"), ("yami-fx.kdl", "rules.kdl"), ("yami-colors.kdl", "colors.kdl"), ("yami-cursor.kdl", "cursor.kdl"), ("yami-outputs.kdl", "outputs.kdl")];

fn mv(from: &Path, to: &Path) {
    if from.exists() && !to.exists() {
        if let Some(d) = to.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::rename(from, to);
    }
}

pub fn migrate() {
    let cfg = yami_px::paths::config_dir();
    let y = yami_dir();
    mv(&cfg.join("yami-shell.json"), &y.join("shell.json"));
    mv(&y.join("modules"), &scripts_dir());
    // сборка темы SDDM: экран входа — yami-greet с 2.6 (yami-cursor не трогать: это рабочая папка
    // установщика, installer/login.sh)
    let _ = std::fs::remove_dir_all(yami_px::paths::cache_dir().join("yami/sddm-yami"));
    let n = cfg.join("niri");
    if !OLD_NIRI.iter().any(|(o, _)| n.join(o).exists()) && !n.join("yami-local.kdl").exists() && !n.join(".yami").exists() {
        return;
    }
    // сначала новая часть, потом правка config.kdl: niri перечитывает его сразу после записи
    let set = crate::config::Settings::load();
    for (old, new) in OLD_NIRI {
        mv(&n.join(old), &niri_dir().join(new));
        // новый уже был: старый не нужен, шелл перепишет новый сам
        let _ = std::fs::remove_file(n.join(old));
    }
    crate::niricfg::write_yami(&set);
    crate::niricfg::migrate_old_config(&n);
    let _ = std::fs::remove_file(n.join("yami-local.kdl"));
    let _ = std::fs::remove_file(n.join(".yami"));
}

/// откат на версию до 3.0: она ищет настройки и скрипты на старых местах. Копия, не перенос:
/// возвращение на новую версию найдёт всё на месте
pub fn back_to_old() {
    let cfg = yami_px::paths::config_dir();
    let y = yami_dir();
    let old = cfg.join("yami-shell.json");
    if !old.exists() {
        let _ = std::fs::copy(y.join("shell.json"), &old);
    }
    if scripts_dir().is_dir() && !y.join("modules").exists() {
        let _ = std::os::unix::fs::symlink("scripts", y.join("modules"));
    }
}
