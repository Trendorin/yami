// Настройки скриншотов: файл ~/.config/yami/shot.json принадлежит yami-shot, вкладка шелла правит его же.
// До 2.8 они лежали в настройках шелла: при первой загрузке переезжают оттуда.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct ShotCfg {
    pub dir: String,
    pub copy: bool,
    pub save: bool,
    pub loupe: bool,
    pub notify: bool,
    pub delay: u32,
    pub confirm: String,
    pub prefix: String,
    /// когда файл прочитан: yami-shot правит его и сам (кнопка таймера), шелл перечитывает перед правкой
    #[serde(skip)]
    mtime: Option<std::time::SystemTime>,
}

impl Default for ShotCfg {
    fn default() -> ShotCfg {
        ShotCfg { dir: String::new(), copy: true, save: true, loupe: true, notify: true, delay: 3, confirm: "space".into(), prefix: "yami".into(), mtime: None }
    }
}

fn path() -> PathBuf {
    crate::config::config_dir().join("yami/shot.json")
}

fn mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

impl ShotCfg {
    pub fn load() -> ShotCfg {
        // файл есть — он главный: негодный ключ берётся по умолчанию, остальное остаётся, файл не переписывается
        if let Ok(t) = std::fs::read_to_string(path()) {
            let mut c: ShotCfg = yami_px::paths::lenient_json(&t);
            c.mtime = mtime();
            return c;
        }
        let old: Value = std::fs::read_to_string(crate::config::settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(Value::Null);
        let d = ShotCfg::default();
        let b = |k: &str, v: bool| old.get(k).and_then(Value::as_bool).unwrap_or(v);
        let mut c = ShotCfg {
            dir: old.get("shotDir").and_then(Value::as_str).unwrap_or("").into(),
            copy: b("shotCopy", d.copy),
            save: b("shotSave", d.save),
            loupe: b("shotLoupe", d.loupe),
            notify: b("shotNotify", d.notify),
            delay: old.get("shotDelay").and_then(Value::as_u64).map_or(d.delay, |v| v as u32),
            confirm: if old.get("shotConfirm").and_then(Value::as_str) == Some("instant") { "instant".into() } else { d.confirm.clone() },
            prefix: d.prefix,
            mtime: None,
        };
        c.save();
        c
    }

    /// файл изменился с прошлого чтения (yami-shot, руками): прочитать заново; true — изменился
    pub fn fresh(&mut self) -> bool {
        if mtime() == self.mtime {
            return false;
        }
        *self = ShotCfg::load();
        true
    }

    pub fn save(&mut self) {
        let _ = crate::config::write_atomic(&path(), &(serde_json::to_string_pretty(self).unwrap_or_default() + "\n"));
        self.mtime = mtime();
    }

    /// куда лягут снимки: для подсказки в поле
    pub fn folder(&self) -> PathBuf {
        let d = self.dir.trim();
        if d.is_empty() {
            yami_px::paths::pictures().join("Screenshots")
        } else {
            yami_px::paths::expand(d)
        }
    }
}
