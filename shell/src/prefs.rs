// Применение настроек: любое изменение (панель, правка shell.json руками, CLI) пересобирает
// тему и шрифты, переписывает то, что читают niri и другие программы, перезапускает процессы шелла.

use crate::app::*;
use crate::config::{self, Settings};
use crate::font::Fonts;


/// пустая папка шаблонов получает пример (выключенный: без строки yami-output он не пишется)
pub fn templates_example(d: &std::path::Path) {
    let _ = std::fs::create_dir_all(d);
    let empty = std::fs::read_dir(d).map(|mut r| r.next().is_none()).unwrap_or(false);
    if empty {
        let ex = "# yami template example: copy this file, put your app's colour config below and add the line\n# «yami-output: ~/.config/APP/colors» (where to write) and, if the app can reload,\n# «yami-reload: USR1 APP». Colours: {{accent}} {{deep}} {{bright}} {{bg}} {{panel}} {{surface}}\n# {{border}} {{text}} {{textDim}} {{textFaint}} {{foreground}} {{background}} {{cursor}} {{color0}}…{{color15}};\n# {{accent.strip}} — without «#», {{accent.rgb}} — «r, g, b». Files ending in .md are skipped.\n";
        let _ = std::fs::write(d.join("example.md"), ex);
    }
}

/// общий вид для модулей. Тот же текст не перезаписывается: модули не перечитывают его зря
pub fn write_style(s: &Settings) {
    let mut v = s.style_value();
    if let Some(o) = v.as_object_mut() {
        o.insert("_".into(), "written by yami-shell (settings → look); what one module does differently goes to style.<module>.json".into());
    }
    let t = serde_json::to_string_pretty(&v).unwrap_or_default() + "\n";
    let _ = config::write_atomic(&config::config_dir().join("yami/style.json"), &t);
}

impl App {
    // ── настройки ───────────────────────────────────────────────────────────
    pub fn settings_changed(&mut self) {
        let s = &self.st.set;
        crate::i18n::set(&s.lang);
        if s.font_family() != self.res.font {
            self.res.font = s.font_family();
            self.res.fonts = Fonts::with(&self.res.font);
        }
        // тот же разбор, что у модулей: шелл и модули не могут разойтись во внешности
        self.st.t = yami_px::look::Look::from_json(&s.style_value(), Some(s.light)).theme;
        crate::prefs::write_style(s);
        // уход карточек уведомлений (настройки → анимации)
        self.st.notifs.leave_s = crate::notif::leave_s(s);
        // набор значков сменился (или «авто» и в системе теперь другой) — собрать заново
        let iset = crate::app::icon_set(&s.icon_theme);
        if iset != self.res.iset {
            self.res.icons = crate::icons::Icons::new(&iset);
            self.res.iset = iset;
        }
        self.res.icons.set_style(self.st.t.icon_ramp(), s.icon_pixel, s.icon_tint);
        self.write_palette();
        self.write_fx();
        self.side_sync();
        crate::agents::write_portals(&self.st.set);
        self.sync_wall();
        crate::keys::write(&self.st.set);
        crate::login::publish(&self.st.set);
        self.setup_idle();
        self.sync_bar_outputs();
        self.sync_widgets();
        self.sync_dock();
        self.sync_bar();
        self.dirty_all();
    }

    /// записать настройки сейчас (перед перезапуском), даже посреди перетаскивания
    pub fn save_now(&mut self) {
        let j = self.st.set.to_json();
        if j != self.set_saved {
            let _ = config::write_atomic(&config::settings_path(), &j);
            self.set_saved = j;
        }
    }

    pub(crate) fn save_settings_if_changed(&mut self) {
        if self.press.is_some() || self.st.set == self.set_seen {
            return;
        }
        self.set_seen = self.st.set.clone();
        let j = self.st.set.to_json();
        if j != self.set_saved {
            let _ = config::write_atomic(&config::settings_path(), &j);
            self.set_saved = j;
            self.set_mtime = std::fs::metadata(config::settings_path()).and_then(|m| m.modified()).ok();
        }
    }

    pub(crate) fn check_settings_file(&mut self) {
        let m = std::fs::metadata(config::settings_path()).and_then(|m| m.modified()).ok();
        if m != self.set_mtime {
            self.set_mtime = m;
            let s = Settings::load();
            if s != self.st.set {
                // правка извне (своя тема, yami config set, руками): шрифты и курсор программ
                // recolor перекрашивает сам только со сменой палитры — их входы сверяем отдельно
                let fonts = |s: &Settings| (s.font_ui.clone(), s.font_mono.clone(), s.font_pt, s.font_size, s.browser_font, s.browser_menus);
                let cursor = |s: &Settings| (s.cursor.clone(), s.cursor_size);
                let (f0, c0, pal0) = (fonts(&self.st.set), cursor(&self.st.set), self.palette_written.clone());
                self.st.set = s;
                self.set_saved = self.st.set.to_json();
                self.settings_changed();
                // новая палитра — recolor уже идёт целиком (шрифты и курсор в нём)
                if self.palette_written == pal0 {
                    if fonts(&self.st.set) != f0 {
                        self.apply_fonts();
                    }
                    if cursor(&self.st.set) != c0 {
                        self.apply_cursor();
                    }
                }
            }
        }
    }

    /// палитра для остальных программ; yami-shell recolor перекрашивает их и рисует обои
    pub fn write_palette(&mut self) {
        let j = self.st.t.palette_json();
        if j == self.palette_written {
            return;
        }
        self.palette_written = j.clone();
        let p = config::state_dir().join("palette.json");
        let changed = std::fs::read_to_string(&p).map(|t| t != j).unwrap_or(true);
        let _ = config::write_atomic(&p, &j);
        if changed || !config::state_dir().join("wallpaper.png").exists() {
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                crate::proc::recolor_run(&[p.as_os_str()]);
                let _ = tx.send(Msg::Theme);
            });
        }
    }

    /// перекрасить программы заново той же палитрой (включили foot или свой шаблон): recolor
    /// читает настройки с диска — сначала сохранить
    pub fn theme_rerun(&mut self) {
        self.save_now();
        let p = config::state_dir().join("palette.json");
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            crate::proc::recolor_run(&[p.as_os_str()]);
            let _ = tx.send(Msg::Theme);
        });
    }

    /// ползунок настроек: значение в настройки, всё зависящее — следом
    pub fn slider_set(&mut self, key: &str, v: f32) {
        if key == "pixel_level" {
            self.set_pixel_level(v.round().clamp(0.0, 3.0) as usize);
            return;
        }
        crate::prefs::settings_set_f32(&mut self.st.set, key, v);
        self.settings_changed();
    }

    /// пиксельность одним ползунком (настройки → вид): шрифты шелла, файлов, просмотра и программ,
    /// пиксельные значки и курсор. 3 — всё пиксельное (как по умолчанию), 2 — только yami (GTK и Qt
    /// гладкие), 1 — мягче (пиксельные только панели шелла), 0 — гладко везде
    pub fn set_pixel_level(&mut self, lvl: usize) {
        if lvl == crate::settabs::pixel_level(&self.st.set) {
            return;
        }
        let (shell, apps, ui, icons, cursor) = match lvl {
            0 => ("sans", "sans", "sans", false, "system"),
            1 => ("", "sans", "sans", false, "yami"),
            2 => ("", "", "sans", true, "yami"),
            _ => ("", "", "", true, "yami"),
        };
        let s = &mut self.st.set;
        // свой набор курсоров (не yami и не system) ползунок не трогает: его выбрали руками
        let cursor = if matches!(s.cursor.as_str(), "yami" | "system" | "") { cursor } else { s.cursor.as_str() }.to_string();
        let (ui_new, cur_new) = (s.font_ui != ui, s.cursor != cursor);
        s.font_shell = shell.into();
        s.font_files = apps.into();
        s.font_view = apps.into();
        s.font_ui = ui.into();
        s.icon_pixel = icons;
        s.cursor = cursor;
        self.settings_changed();
        // программы (GTK, Qt) и курсор перекрашивает recolor — только если они правда сменились,
        // и не на каждое деление, пока ползунок тянут: один раз при отпускании (look_apply)
        self.look_pending.0 |= ui_new;
        self.look_pending.1 |= cur_new;
        if self.press.is_none() {
            self.look_apply();
        }
    }

    /// отложенное от ползунка пиксельности: шрифты программ и курсор
    pub fn look_apply(&mut self) {
        let (fonts, cursor) = std::mem::take(&mut self.look_pending);
        if fonts {
            self.apply_fonts();
        }
        if cursor {
            self.apply_cursor();
        }
    }

    /// курсор сменили в настройках: recolor --cursor (niri перечитает свой конфиг сам)
    pub fn apply_cursor(&mut self) {
        // настройки должны быть на диске до запуска скрипта
        let j = self.st.set.to_json();
        let _ = config::write_atomic(&config::settings_path(), &j);
        self.set_saved = j;
        std::thread::spawn(|| {
            crate::proc::recolor_run(&["--cursor".as_ref()]);
        });
    }

    /// размер шрифта программ сменили (или экраны): recolor --fonts — GTK, Qt, kitty, браузер
    pub fn apply_fonts(&mut self) {
        let j = self.st.set.to_json();
        let _ = config::write_atomic(&config::settings_path(), &j);
        self.set_saved = j;
        std::thread::spawn(|| {
            crate::proc::recolor_run(&["--fonts".as_ref()]);
        });
    }

    /// процессы при шелле: ночной свет, модули-скрипты — по настройкам
    pub fn side_sync(&mut self) {
        self.night_sync();
        self.privacy_sync();
        let s = &self.st.set;
        let mut on_bar: Vec<String> = s.bar_left.iter().chain(&s.bar_center).chain(&s.bar_right).cloned().collect();
        // виджет-скрипт на столе — тот же модуль, запущенный один раз
        on_bar.extend(s.widgets.iter().filter(|w| w.on && w.kind == "script" && !w.source.is_empty()).map(|w| format!("x:{}", w.source)));
        on_bar.dedup();
        self.st.plugins.sync(&on_bar, &self.tx);
    }

    /// rules.kdl и yami.kdl — только если другие: каждая запись — перечитывание конфига niri
    pub fn write_fx(&mut self) {
        crate::niricfg::write_yami(&self.st.set);
        let p = crate::dirs::niri_dir().join("rules.kdl");
        let text = self.st.set.fx_kdl(crate::niri::config_level(self.st.niri.level), self.st.curtain.is_some());
        if std::fs::read_to_string(&p).ok().as_deref() != Some(text.as_str()) {
            let _ = config::write_atomic(&p, &text);
        }
    }
}

/// ползунки настроек (SSlider): число в поле Settings по ключу и обратно
pub fn settings_set_f32(s: &mut Settings, key: &str, v: f32) {
    match key {
        "tint" => s.tint = (v * 10.0).round() / 10.0,
        "glass" => s.glass = (v * 100.0).round() / 100.0,
        "lift" => s.lift = (v * 100.0).round() / 100.0,
        "inner" => s.inner = (v * 100.0).round() / 100.0,
        "dim" => s.dim = (v * 100.0).round() / 100.0,
        "anim_speed" => s.anim_speed = (v * 10.0).round() / 10.0,
        "float_w" => s.float_w = (v * 20.0).round() / 20.0,
        "dock_size" => s.dock_size = ((v / 4.0).round() * 4.0) as i32,
        "float_h" => s.float_h = (v * 20.0).round() / 20.0,
        "lock_after" => s.lock_after = v.round() as u32,
        "screen_off_after" => s.screen_off_after = v.round() as u32,
        "notif_timeout" => s.notif_timeout = v.round().max(2.0) as u32,
        "task_width" => s.task_width = (v / 10.0).round() as i32 * 10,
        "wall_pixel" => s.wall_pixel = v.round().max(1.0) as i32,
        "wall_dim" => s.wall_dim = (v * 20.0).round() / 20.0,
        "greet_dim" => s.greet_dim = ((v * 20.0).round() / 20.0).clamp(0.2, 0.9),
        "greet_off" => s.greet_off = v.round() as u32,
        "curtain_cell" => s.curtain_cell = ((v / 4.0).round() * 4.0) as i32,
        _ => {}
    }
}

pub fn settings_get_f32(s: &Settings, key: &str) -> f32 {
    match key {
        "pixel_level" => crate::settabs::pixel_level(s) as f32,
        "tint" => s.tint,
        "glass" => s.glass,
        "lift" => s.lift,
        "inner" => s.inner,
        "dim" => s.dim,
        "anim_speed" => s.anim_speed,
        "float_w" => s.float_w,
        "dock_size" => s.dock_size as f32,
        "float_h" => s.float_h,
        "lock_after" => s.lock_after as f32,
        "screen_off_after" => s.screen_off_after as f32,
        "notif_timeout" => s.notif_timeout as f32,
        "task_width" => s.task_width as f32,
        "wall_pixel" => s.wall_pixel as f32,
        "wall_dim" => s.wall_dim,
        "greet_dim" => s.greet_dim,
        "greet_off" => s.greet_off as f32,
        "curtain_cell" => s.curtain_cell as f32,
        _ => 0.0,
    }
}

/// kitty снова в цветах палитры: наш yami.conf рядом и одна строка include в конце kitty.conf (как у
/// установщика); остальной kitty.conf — человека
pub fn kitty_attach() {
    let dir = config::config_dir().join("kitty");
    let _ = std::fs::create_dir_all(&dir);
    let _ = config::write_atomic(&dir.join("yami.conf"), include_str!("../../config/kitty/yami.conf"));
    let conf = dir.join("kitty.conf");
    let t = std::fs::read_to_string(&conf).unwrap_or_default();
    if !t.lines().any(|l| l.trim() == "include yami.conf") {
        let sep = if t.is_empty() || t.ends_with('\n') { "" } else { "\n" };
        let text = format!("{t}{sep}\n# YAMI: settings → color → apps → kitty off takes this line out\ninclude yami.conf\n");
        // ссылка (stow, Home Manager) остаётся ссылкой: пишем сквозь неё, а не подменяем файлом
        let _ = if conf.is_symlink() { std::fs::write(&conf, text) } else { config::write_atomic(&conf, &text) };
    }
}
