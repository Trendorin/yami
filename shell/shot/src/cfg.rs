// Настройки yami-shot: ~/.config/yami/shot.json (вид — отдельно, style.json и style.shot.json).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use yami_px::paths;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Cfg {
    /// папка снимков; пусто — «Изображения/Screenshots»
    pub dir: String,
    pub copy: bool,
    pub save: bool,
    pub loupe: bool,
    pub notify: bool,
    /// секунды таймера (кнопка ◷)
    pub delay: u32,
    /// space — готово по пробелу или enter; instant — как только отпустил мышь
    pub confirm: String,
    /// начало имени файла
    pub prefix: String,
}

impl Default for Cfg {
    fn default() -> Cfg {
        Cfg { dir: String::new(), copy: true, save: true, loupe: true, notify: true, delay: 3, confirm: "space".into(), prefix: "yami".into() }
    }
}

fn path() -> PathBuf {
    paths::config_dir().join("yami/shot.json")
}

impl Cfg {
    pub fn load() -> Cfg {
        std::fs::read_to_string(path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = paths::write_atomic(&path(), &(serde_json::to_string_pretty(self).unwrap_or_default() + "\n"));
    }

    pub fn folder(&self) -> PathBuf {
        let d = self.dir.trim();
        if d.is_empty() {
            paths::pictures().join("Screenshots")
        } else {
            paths::expand(d)
        }
    }

    pub fn instant(&self) -> bool {
        self.confirm == "instant"
    }
}
