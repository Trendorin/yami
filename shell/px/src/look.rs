// Вид модуля: цвета, стиль, шрифт, язык, анимации. Источники по порядку:
//   ~/.config/yami/style.json        общий вид всех модулей (его пишет шелл, первая настройка или человек);
//   ~/.config/yami/style.<app>.json  свой вид модуля, только отличия;
//   нет style.json, но есть настройки шелла — они (шелл ещё не успел выложить файл);
//   нет ничего — пресет plain: нейтральный вид, светлая или тёмная тема как в системе.
// Файлы перечитываются, когда меняются: модули перекрашиваются без перезапуска.

use crate::style::{self, Corners, Shadow, Style};
use crate::theme::{parse_hex, Theme, PRESETS};
use serde_json::{Map, Value};
use std::time::SystemTime;

/// акцент нейтрального вида, если свой не задан
const PLAIN_ACCENT: u32 = 0x3584e4;

#[derive(Clone)]
pub struct Look {
    pub theme: Theme,
    pub style: Style,
    /// "" (по языку системы), "ru", "en", "pt-BR"
    pub lang: String,
    pub animations: bool,
    pub anim_speed: f32,
    pub icon_theme: String,
    pub blur: bool,
    /// значки программ пиксельными в палитре
    pub icon_pixel: bool,
    /// пиксельные значки в тонах палитры, а не в своих цветах
    pub icon_tint: bool,
    /// шрифты: шелла и отдельно файлов и просмотрщика ("" — как у шелла)
    pub font_shell: String,
    pub font_files: String,
    pub font_view: String,
    /// команда «сделать обоями» вне шелла yami: {} заменяется на путь (пусто — пункта нет)
    pub wallpaper_cmd: String,
    /// нейтральный пресет: у него свой шрифт по умолчанию
    plain: bool,
    /// какой модуль читает (style.<app>.json)
    app: String,
    stamp: [Option<SystemTime>; 3],
}

fn mtime(p: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn stamp(app: &str) -> [Option<SystemTime>; 3] {
    [mtime(&crate::paths::style_file()), mtime(&crate::paths::style_app_file(app)), mtime(&crate::paths::shell_settings())]
}

fn read_json(p: &std::path::Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

/// светлая ли тема в системе: настройка GTK, затем gsettings; ничего не нашлось — тёмная
fn system_light() -> bool {
    let cfg = crate::paths::config_dir();
    for v in ["gtk-4.0", "gtk-3.0"] {
        let Ok(t) = std::fs::read_to_string(cfg.join(v).join("settings.ini")) else { continue };
        for l in t.lines() {
            let Some((k, val)) = l.split_once('=') else { continue };
            let val = val.trim().to_lowercase();
            match k.trim() {
                "gtk-application-prefer-dark-theme" => return val == "0" || val == "false",
                "gtk-theme-name" if val.contains("dark") => return false,
                _ => {}
            }
        }
    }
    let out = std::process::Command::new("gsettings").args(["get", "org.gnome.desktop.interface", "color-scheme"]).stderr(std::process::Stdio::null()).output();
    out.is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("prefer-light"))
}

/// own поверх base: ключ заменяется целиком, только цвета сливаются по одному
fn merge(base: &mut Map<String, Value>, own: Map<String, Value>) {
    for (k, v) in own {
        match (k.as_str(), base.get_mut(&k), v) {
            ("colors", Some(Value::Object(b)), Value::Object(o)) => b.extend(o),
            (_, _, v) => {
                base.insert(k, v);
            }
        }
    }
}

impl Look {
    pub fn load(app: &str) -> Look {
        Look::load_mode(app, None)
    }

    /// light: Some — тема поверх настроек (снимки `shot --theme light`)
    pub fn load_mode(app: &str, light: Option<bool>) -> Look {
        let shared = read_json(&crate::paths::style_file())
            .or_else(|| read_json(&crate::paths::shell_settings()))
            .unwrap_or_else(|| serde_json::json!({ "preset": "plain" }));
        let mut map = match shared {
            Value::Object(m) => m,
            _ => Map::new(),
        };
        if let Some(Value::Object(own)) = read_json(&crate::paths::style_app_file(app)) {
            merge(&mut map, own);
        }
        let mut l = Look::from_json(&Value::Object(map), light);
        l.app = app.to_string();
        l.stamp = stamp(app);
        l
    }

    /// вид из JSON: style.json, настройки шелла (там нет ключа preset — значит, вид yami) или копия,
    /// которую шелл выкладывает экрану входа
    pub fn from_json(v: &Value, light: Option<bool>) -> Look {
        let f = |k: &str, d: f32| v.get(k).and_then(|x| x.as_f64()).map(|x| x as f32).unwrap_or(d);
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let b = |k: &str, d: bool| v.get(k).and_then(|x| x.as_bool()).unwrap_or(d);
        let plain = s("preset") == "plain";
        let base = if plain { Style::PLAIN } else { Style::PIXEL };
        let style = Style {
            corners: Corners::from_id(&s("corners")).unwrap_or(base.corners),
            radius: v.get("radius").and_then(|x| x.as_i64()).map(|x| x.clamp(0, 64) as u8).unwrap_or(base.radius),
            shadow: Shadow::from_id(&s("shadow")).unwrap_or(base.shadow),
            bevel: b("bevel", base.bevel),
            cells: b("cells", base.cells),
            smooth: b("smooth", base.smooth),
        };
        style::set(style);
        let reveal = s("reveal");
        let reveal = if reveal.is_empty() && plain { "fade" } else { &reveal };
        crate::gfx::set_reveal(crate::gfx::Reveal::from_id(reveal), v.get("revealGrain").and_then(|x| x.as_i64()).unwrap_or(1) as i32);
        let palette = s("palette");
        let accent = parse_hex(&s("accent")).unwrap_or_else(|| match PRESETS.iter().find(|p| p.id == palette) {
            Some(p) => p.accent,
            None if plain => PLAIN_ACCENT,
            None => PRESETS[0].accent,
        });
        let is_light = light.or_else(|| v.get("light").and_then(|x| x.as_bool())).unwrap_or_else(|| plain && system_light());
        let mut theme = Theme::new(accent, f("tint", if plain { 0.6 } else { 1.0 }), f("glass", if plain { 1.0 } else { 0.55 }), f("lift", if plain { 0.25 } else { 0.0 }), f("inner", 1.0), is_light);
        if let Some(c) = v.get("colors").and_then(|x| x.as_object()) {
            theme.override_colors(c);
        }
        let lite = b("lite", false);
        let font = s("font");
        Look {
            theme: theme.apply(),
            style,
            lang: s("lang"),
            // лёгкий режим шелла (слабая или виртуальная машина) выключает анимации и здесь
            animations: b("animations", true) && !lite,
            anim_speed: f("animSpeed", 1.0),
            icon_theme: s("iconTheme"),
            blur: b("blur", b("blurShell", !plain)) && !lite,
            icon_pixel: b("iconPixel", !plain),
            icon_tint: b("iconTint", false),
            font_shell: if font.is_empty() { s("fontShell") } else { font },
            font_files: s("fontFiles"),
            font_view: s("fontView"),
            wallpaper_cmd: s("wallpaperCmd"),
            plain,
            app: String::new(),
            stamp: [None; 3],
        }
    }

    /// шрифт приложения: свой или общий; пустой общий — Tiny5 у вида yami и системный у нейтрального
    pub fn font(&self, app: &str) -> &str {
        let own = match app {
            "files" => &self.font_files,
            "view" => &self.font_view,
            _ => &self.font_shell,
        };
        match (own.is_empty(), self.font_shell.is_empty()) {
            (false, _) => own,
            (true, false) => &self.font_shell,
            (true, true) if self.plain => "sans",
            (true, true) => "",
        }
    }

    /// настройки вида изменились с прошлой загрузки
    pub fn stale(&self) -> bool {
        stamp(&self.app) != self.stamp
    }

    /// длительность анимации с учётом скорости из настроек; 0 — анимации выключены
    pub fn dur(&self, ms: f32) -> f32 {
        if self.animations {
            ms * self.anim_speed.max(0.1)
        } else {
            0.0
        }
    }

    /// можно ли сделать картинку обоями: шелл yami или своя команда в style.json
    pub fn can_set_wallpaper(&self) -> bool {
        crate::paths::shell_settings().exists() || !self.wallpaper_cmd.is_empty()
    }
}

/// «сделать обоями» из файлов и просмотрщика. Есть настройки шелла — путь в них, остальное не трогаем
/// (шелл замечает новый файл настроек сам и готовит картинку, wall.rs). Нет шелла — своя команда
/// из style.json («swww img {}»); путь уходит отдельным аргументом, через оболочку не идёт
pub fn set_wallpaper(p: &std::path::Path, cmd: &str) -> bool {
    let abs = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let path = crate::paths::shell_settings();
    if let Some(mut v) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()) {
        let Some(o) = v.as_object_mut() else { return false };
        o.insert("wallpaper".into(), Value::String(abs.to_string_lossy().to_string()));
        return crate::paths::write_atomic(&path, &(serde_json::to_string_pretty(&v).unwrap_or_default() + "\n")).is_ok();
    }
    let mut args = cmd.split_whitespace();
    let Some(prog) = args.next() else { return false };
    let a: Vec<std::ffi::OsString> = args.map(|x| if x == "{}" { abs.clone().into_os_string() } else { x.into() }).collect();
    std::process::Command::new(prog).args(a).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn settings_without_a_preset_keep_the_yami_look() {
        let l = Look::from_json(&json!({ "palette": "ash" }), Some(false));
        assert_eq!(l.style, Style::PIXEL);
        assert_eq!(l.font("files"), "");
        assert!(l.icon_pixel);
    }

    #[test]
    fn plain_is_neutral_and_every_part_can_be_changed() {
        let l = Look::from_json(&json!({ "preset": "plain" }), Some(false));
        assert_eq!(l.style, Style::PLAIN);
        assert_eq!(l.font("files"), "sans");
        assert!(!l.icon_pixel);
        let l = Look::from_json(&json!({ "preset": "plain", "corners": "square", "shadow": "none", "radius": 99, "font": "Inter" }), Some(false));
        assert_eq!((l.style.corners, l.style.shadow, l.style.radius), (Corners::Square, Shadow::Off, 64));
        assert_eq!(l.font("view"), "Inter");
    }

    #[test]
    fn own_colours_win_and_merge_key_by_key() {
        let mut base = json!({ "colors": { "bg": "#101010", "text": "#eeeeee" }, "radius": 4 }).as_object().unwrap().clone();
        let own = json!({ "colors": { "bg": "#202020" }, "radius": 12 }).as_object().unwrap().clone();
        merge(&mut base, own);
        let l = Look::from_json(&Value::Object(base), Some(false));
        assert_eq!(l.style.radius, 12);
        assert_eq!(l.theme.bg.hex_str().to_lowercase(), "#202020");
        assert_eq!(l.theme.text.hex_str().to_lowercase(), "#eeeeee");
    }
}
