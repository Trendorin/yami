// Настройки yami-agents: ~/.config/yami/agents.json — какие части работают. До 2.8 часть из них
// лежала в настройках шелла: при первой загрузке переезжают оттуда.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use yami_px::paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Cfg {
    /// окно пароля polkit (выкл — свой агент: polkit-gnome, KDE)
    pub polkit: bool,
    /// окно «что показать» и бэкенд показа экрана (нужен niri)
    pub screencast: bool,
    /// флешки, диски, телефоны и камеры подключаются сами
    pub automount: bool,
    /// подключённое сразу открывать в файлах, а не показывать уведомление
    pub automount_open: bool,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg { polkit: true, screencast: true, automount: true, automount_open: false }
    }
}

fn path() -> PathBuf {
    paths::config_dir().join("yami/agents.json")
}

impl Cfg {
    pub fn load() -> Cfg {
        if let Some(c) = std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()) {
            return c;
        }
        let d = Cfg::default();
        let Some(old) = std::fs::read_to_string(paths::shell_settings()).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) else { return d };
        let b = |k: &str, v: bool| old.get(k).and_then(Value::as_bool).unwrap_or(v);
        let c = Cfg { polkit: b("agentPolkit", d.polkit), screencast: b("portalCast", d.screencast), automount: b("automount", d.automount), automount_open: b("automountOpen", d.automount_open) };
        c.save();
        c
    }

    pub fn save(&self) {
        let _ = paths::write_atomic(&path(), &(serde_json::to_string_pretty(self).unwrap_or_default() + "\n"));
    }

    /// ключ из командной строки: имя части → её переключатель
    pub fn slot(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "polkit" => &mut self.polkit,
            "screencast" => &mut self.screencast,
            "automount" => &mut self.automount,
            "automount-open" => &mut self.automount_open,
            _ => return None,
        })
    }
}
