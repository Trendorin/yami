// Страницы мастера первой настройки (welcome.rs): одна тема на экран, крупные карточки с
// миниатюрами. Тонкие настройки — за кнопкой «все настройки» того же шага: это вкладки настроек.
// Выборы, которых нет среди действий настроек, — Act::WSet, разбирает `set` внизу файла.

use crate::app::{App, State};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::settings::{num_btn, opt_btn, title};
use crate::theme::PRESETS;
use crate::ui::{Act, Cur, Tone, Ui};
use crate::welcome::Welcome;
use crate::widgets::KINDS;
use yami_px::style::Style;

/// карточка выбора: сверху миниатюра, снизу имя и пояснение; выбранная — в рамке акцента
fn pick(ui: &mut Ui, r: R, name: &str, desc: &str, on: bool, act: Act, art: impl FnOnce(&mut Ui, R)) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if on { t.surface_hi } else { t.surface }, if on { t.red } else if hov { t.ember } else { t.border }).shadow(2));
    let text_h = if desc.is_empty() { 26 } else { 46 };
    let pv = R::new(r.x + 10, r.y + 10, r.w - 20, r.h - 18 - text_h);
    art(ui, pv);
    let ty = pv.bottom() + 6;
    ui.cv.fill(R::new(r.x + 12, ty + 6, 8, 8), if on { t.red } else { t.border });
    ui.text_in(PX, R::new(r.x + 28, ty, r.w - 38, 20), name, if on { t.text_hi } else { t.text });
    if !desc.is_empty() {
        ui.text_in(PX, R::new(r.x + 12, ty + 20, r.w - 24, 20), desc, t.text_faint);
    }
    ui.zone(r, act, Cur::Pointer);
}

/// карточка без миниатюры: значок, имя, пояснение
fn card(ui: &mut Ui, r: R, icon: &str, name: &str, desc: &str, on: bool, act: Act) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if on { t.surface_hi } else { t.surface }, if on { t.red } else if hov { t.ember } else { t.border }).shadow(2));
    ui.cv.icon(icon, r.x + 14, r.y + (r.h - 21) / 2, 3, if on { t.red } else { t.text_dim });
    ui.text_in(PX, R::new(r.x + 50, r.y + (r.h - 42) / 2, r.w - 60, 20), name, if on { t.text_hi } else { t.text });
    ui.text_in(PX, R::new(r.x + 50, r.y + (r.h - 42) / 2 + 20, r.w - 60, 20), desc, t.text_faint);
    ui.zone(r, act, Cur::Pointer);
}

/// миниатюра стола в заданном стиле: бар и окно. Стиль меняется только на время рисования
fn art_style(ui: &mut Ui, r: R, style: Style) {
    let t = ui.t;
    let was = ui.cv.style;
    ui.cv.style = style;
    ui.cv.fill(r, t.bg);
    ui.cv.fill(R::new(r.x, r.y, r.w, 9), t.panel);
    ui.cv.fill(R::new(r.x + 4, r.y + 3, 14, 3), t.red);
    let w = R::new(r.x + r.w / 6, r.y + 20, r.w * 2 / 3, r.h - 30);
    ui.cv.pxbox(w, &BoxStyle::new(t.surface, t.border).shadow(3));
    ui.cv.fill(R::new(w.x + 10, w.y + 9, w.w / 2, 4), t.red);
    for i in 0..3 {
        ui.cv.fill(R::new(w.x + 10, w.y + 21 + i * 10, (w.w - 20 - i * 16).max(8), 3), t.text_faint);
    }
    ui.cv.style = was;
}

fn art_bar(ui: &mut Ui, r: R, shape: u8) {
    let t = ui.t;
    ui.cv.fill(r, t.bg);
    let bar = match shape {
        0 => R::new(r.x, r.y, r.w, 10),
        1 => R::new(r.x + r.w / 6, r.y, r.w * 2 / 3, 10),
        _ => R::new(r.x + 6, r.y + 5, r.w - 12, 10),
    };
    ui.cv.fill(bar, t.surface_hi);
    ui.cv.fill(R::new(bar.x + 4, bar.y + 3, 12, 4), t.red);
    ui.cv.fill(R::new(bar.right() - 16, bar.y + 3, 12, 4), t.text_faint);
    ui.cv.fill(R::new(r.x + r.w / 2 - 22, r.y + 26, 44, r.h - 34), t.surface);
}

/// сколько всего на баре: фишки разной ширины
fn art_mods(ui: &mut Ui, r: R, n: i32) {
    let t = ui.t;
    ui.cv.fill(r, t.surface_hi);
    let w = ((r.w - 8) / 14).max(4);
    for i in 0..n {
        let x = if i < n / 2 { r.x + 4 + i * (w + 3) } else { r.right() - 4 - (n - i) * (w + 3) };
        ui.cv.fill(R::new(x, r.y + r.h / 2 - 3, w, 6), if i == 0 { t.red } else { t.text_faint });
    }
}

fn art_dock(ui: &mut Ui, r: R, side: u8) {
    let t = ui.t;
    ui.cv.fill(r, t.bg);
    let n = 4;
    let (cell, gap) = (8, 3);
    let len = n * cell + (n - 1) * gap + 8;
    let d = match side {
        0 => R::new(r.x + (r.w - len) / 2, r.bottom() - 14, len, 12),
        1 => R::new(r.x + 2, r.y + (r.h - len) / 2, 12, len),
        _ => R::new(r.right() - 14, r.y + (r.h - len) / 2, 12, len),
    };
    ui.cv.fill(d, t.surface_hi);
    for i in 0..n {
        let o = 4 + i * (cell + gap);
        let c = R::new(if side == 0 { d.x + o } else { d.x + 2 }, if side == 0 { d.y + 2 } else { d.y + o }, cell, cell);
        ui.cv.fill(c, if i == 0 { t.red } else { t.text_faint });
    }
}

/// окна лентой: колонки встык, последняя уходит за край
fn art_tiling(ui: &mut Ui, r: R) {
    let t = ui.t;
    ui.cv.fill(r, t.bg);
    ui.cv.push_clip(r);
    let w = r.w * 3 / 8;
    for i in 0..3 {
        let c = R::new(r.x + 6 + i * (w + 5), r.y + 8, w, r.h - 16);
        ui.cv.pxbox(c, &BoxStyle::new(if i == 0 { t.surface_hi } else { t.surface }, if i == 0 { t.red } else { t.border }));
    }
    ui.cv.pop_clip();
}

/// окна, как на обычном столе: внахлёст
fn art_float(ui: &mut Ui, r: R) {
    let t = ui.t;
    ui.cv.fill(r, t.bg);
    let (w, h) = (r.w * 7 / 16, r.h * 5 / 8);
    for (i, (dx, dy)) in [(r.w / 3, 6), (r.w / 8, r.h / 3), (r.w / 12, 8)].into_iter().enumerate() {
        let top = i == 1;
        ui.cv.pxbox(R::new(r.x + dx, r.y + dy, w, h), &BoxStyle::new(if top { t.surface_hi } else { t.surface }, if top { t.red } else { t.border }).shadow(2));
    }
}

/// лесенка из клеток: чем крупнее клетка, тем пиксельнее
fn art_level(ui: &mut Ui, r: R, lvl: i32) {
    let t = ui.t;
    ui.cv.fill(r, t.bg);
    let cell = lvl + 1;
    let (mut x, mut y) = (r.x + 8, r.bottom() - 8 - cell);
    while x < r.right() - 8 && y > r.y + 4 {
        ui.cv.fill(R::new(x, y, cell * 2, cell), t.red);
        x += cell * 2;
        y -= cell;
    }
}

fn art_widget(ui: &mut Ui, r: R, kind: &str) {
    let t = ui.t;
    ui.cv.fill(r, t.bg);
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    match kind {
        "clock" => {
            ui.text_center(crate::font::PX24, r, "12:34", t.text_hi);
        }
        "graph" => {
            for (i, v) in [8, 14, 6, 18, 12, 22, 16].into_iter().enumerate() {
                ui.cv.fill(R::new(cx - 28 + i as i32 * 8, cy + 12 - v, 6, v), if i == 5 { t.red } else { t.text_faint });
            }
        }
        "net" => {
            let mut y = cy + 8;
            for (i, d) in [-4, 2, -6, 4, -8, 3, -2, 1].into_iter().enumerate() {
                ui.cv.fill(R::new(cx - 32 + i as i32 * 8, y, 8, 3), t.red);
                y += d;
            }
        }
        "media" => ui.cv.icon("play", cx - 11, cy - 11, 3, t.red),
        "cal" => {
            for i in 0..21 {
                let on = i == 10;
                ui.cv.fill(R::new(cx - 27 + (i % 7) * 8, cy - 11 + (i / 7) * 8, 6, 6), if on { t.red } else { t.text_faint });
            }
        }
        "weather" => ui.cv.icon("sun", cx - 11, cy - 11, 3, t.red),
        _ => ui.cv.icon("grid", cx - 11, cy - 11, 3, t.text_dim),
    }
}

pub fn hello(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let mut y = b.y + 6;
    let cur = if s.lang.is_empty() {
        if yami_px::i18n::pt_br() {
            "pt-BR"
        } else if yami_px::i18n::translated() {
            "en"
        } else {
            "ru"
        }
    } else {
        s.lang.as_str()
    };
    y = title(ui, b.x, y, b.w, "язык");
    let bw = (b.w - 24) / 3;
    // названия языков — на своём языке, не переводятся
    for (i, (code, name)) in [("ru", "Русский"), ("en", "English"), ("pt-BR", "Português (Brasil)")].into_iter().enumerate() { // i18n-ok
        let r = R::new(b.x + i as i32 * (bw + 12), y, bw, 60);
        let (on, hov) = (cur == code, ui.hover(r));
        ui.cv.pxbox(r, &BoxStyle::new(if on { t.surface_hi } else { t.surface }, if on { t.red } else if hov { t.ember } else { t.border }).shadow(2));
        ui.cv.text_in(ui.f, PX, R::new(r.x + 14, r.y, r.w - 24, r.h), name, if on { t.text_hi } else { t.text });
        ui.zone(r, Act::WLang(code), Cur::Pointer);
    }
    y += 82;
    y = title(ui, b.x, y, b.w, "что будет дальше");
    let half = (b.w - 12) / 2;
    let rows = [("drop", "вид", "стиль, цвет, обои, анимации"), ("window", "рабочий стол", "бар, док, окна, виджеты"), ("keys", "управление", "клавиши и подключение к niri"), ("gear", "система", "программы и службы")];
    for (i, (icon, name, what)) in rows.into_iter().enumerate() {
        let r = R::new(b.x + (i as i32 % 2) * (half + 12), y + (i as i32 / 2) * 72, half, 62);
        ui.cv.pxbox(r, &BoxStyle::new(t.surface, t.border));
        ui.cv.icon(icon, r.x + 14, r.y + 20, 3, t.red);
        ui.text_in(PX, R::new(r.x + 50, r.y + 11, r.w - 60, 20), name, t.text_hi);
        ui.text_in(PX, R::new(r.x + 50, r.y + 31, r.w - 60, 20), what, t.text_faint);
    }
}

pub fn style(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let cw = (b.w - 16) / 2;
    let plain = s.preset == "plain";
    pick(ui, R::new(b.x, b.y, cw, 258), "пиксельный", "целые пиксели, ступенчатые углы", !plain, Act::WPlain(false), |ui, r| art_style(ui, r, Style::PIXEL));
    pick(ui, R::new(b.x + cw + 16, b.y, cw, 258), "нейтральный", "скругления, мягкие тени, обычные шрифты", plain, Act::WPlain(true), |ui, r| art_style(ui, r, Style::PLAIN));
    let y = b.y + 274;
    card(ui, R::new(b.x, y, cw, 84), "moon", "тёмная", "светлый текст на тёмном", !s.light, Act::SCycle("theme_dark"));
    card(ui, R::new(b.x + cw + 16, y, cw, 84), "sun", "светлая", "тёмный текст на светлом", s.light, Act::SCycle("theme_light"));
}

pub fn color(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let n = PRESETS.len() as i32;
    let cw = (b.w - (n - 1) * 8) / n;
    for (i, p) in PRESETS.iter().enumerate() {
        let r = R::new(b.x + i as i32 * (cw + 8), b.y, cw, 150);
        let (on, hov) = (s.accent.is_empty() && s.palette == p.id, ui.hover(r));
        ui.cv.pxbox(r, &BoxStyle::new(if on { t.surface_hi } else { t.surface }, if on { t.red } else if hov { t.ember } else { t.border }).shadow(2));
        let c = crate::gfx::C(0xff000000 | p.accent);
        ui.cv.pxbox(R::new(r.x + 8, r.y + 8, r.w - 16, 104), &BoxStyle::new(c, c.lerp(crate::theme::WHITE, 0.3)).bevel(true));
        ui.text_center(PX, R::new(r.x, r.y + 118, r.w, 24), p.name, if on { t.text_hi } else { t.text_dim });
        ui.zone(r, Act::SPreset(i), Cur::Pointer);
    }
    let mut y = b.y + 168;
    let ph = if s.wall_skwd && s.wall_skwd_colors { "палитру ведёт skwd-wall" } else { "акцент из картинки, с новыми обоями — заново" };
    ui.toggle(R::new(b.x, y, b.w, 40), "палитра из обоев", ph, s.wall_accent, Act::SWall("accent"));
    y += 54;
    y = title(ui, b.x, y, b.w, "так это выглядит");
    let tones = [t.bg, t.panel, t.surface, t.surface_hi, t.border, t.ember, t.red, t.text_faint, t.text_dim, t.text, t.text_hi];
    let tw = (b.w - (tones.len() as i32 - 1) * 4) / tones.len() as i32;
    for (i, c) in tones.into_iter().enumerate() {
        ui.cv.pxbox(R::new(b.x + i as i32 * (tw + 4), y, tw, 86), &BoxStyle::new(c, t.border));
    }
}

pub fn wall(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let pv = R::new(b.x, b.y, 424, 300);
    crate::wall::preview(ui, st, pv);
    let (rx, rw) = (pv.right() + 16, b.right() - pv.right() - 16);
    let mut y = b.y + 2;
    ui.button(R::new(rx, y, rw, 34), "выбрать картинку…", "folder", Tone::Primary, false, true, Act::SWall("pick"));
    y += 44;
    ui.button(R::new(rx, y, rw, 34), "узор из палитры", "image", Tone::Plain, false, crate::wall::own(s).is_some(), Act::SWall("reset"));
    y += 62;
    ui.toggle(R::new(rx, y, rw, 40), "палитра из обоев", "цвет — из картинки", s.wall_accent, Act::SWall("accent"));
    y += 48;
    ui.toggle(R::new(rx, y, rw, 40), "иероглиф на обоях", "в правом нижнем углу", s.wall_kanji, Act::SBool("wall_kanji"));
}

pub fn look(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let mut y = b.y;
    if s.preset != "plain" {
        let lvl = crate::settabs::pixel_level(s);
        let cw = (b.w - 36) / 4;
        for (i, name) in crate::settabs::PIXEL_LEVELS.iter().enumerate() {
            pick(ui, R::new(b.x + i as i32 * (cw + 12), y, cw, 196), name, "", lvl == i, Act::WSet(["level0", "level1", "level2", "level3"][i]), |ui, r| art_level(ui, r, i as i32));
        }
        y += 214;
    }
    let half = (b.w - 20) / 2;
    ui.toggle(R::new(b.x, y, half, 40), "размытие под панелями", "стекло за шторками бара и меню", s.blur_shell, Act::SBool("blur_shell"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "размытие под терминалами", "окна терминала видны насквозь", s.blur_term, Act::SBool("blur_term"));
    y += 50;
    ui.toggle(R::new(b.x, y, half, 40), "лёгкий режим", "слабый ПК, виртуалка: без размытия и анимаций", s.lite, Act::SBool("lite"));
    ui.toggle(R::new(b.x + half + 20, y, half, 40), "строки развёртки", "панели как на старом мониторе", s.scanlines, Act::SBool("scanlines"));
}

pub fn anim(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let sw = 304;
    crate::animtab::sample(ui, st, 0, R::new(b.x, b.y, sw, 256));
    ui.text_center(PX, R::new(b.x, b.y + 264, sw, 20), "щелчок покажет, как это выглядит", t.text_faint);
    let (x, w) = (b.x + sw + 24, b.w - sw - 24);
    let mut y = b.y;
    ui.toggle(R::new(x, y, w, 26), "анимации", "", s.animations, Act::SBool("animations"));
    y += 36;
    ui.toggle(R::new(x, y, w, 26), "анимация закрытия", "", s.anim_close, Act::SBool("anim_close"));
    y += 42;
    ui.slider(R::new(x, y, w, 40), "скорость (меньше — быстрее)", (s.anim_speed - 0.3) / 1.7, &format!("×{:.1}", s.anim_speed), 17, Act::SSlider("anim_speed", 0.3, 2.0, 17));
    y += 52;
    opt_btn(ui, R::new(x, y, w, 26), "узор", crate::settabs::reveal_name(&s.reveal), Act::SCycle("reveal"));
    if s.lite {
        ui.text_in(PX, R::new(b.x, b.y + 296, b.w, 20), "лёгкий режим: анимаций нет", t.warn);
    }
}

pub fn bar(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let cw = (b.w - 24) / 3;
    let col = |i: i32| b.x + i * (cw + 12);
    let strip = crate::bar::strip(s);
    let mut y = b.y;
    pick(ui, R::new(col(0), y, cw, 160), "полоса", "во всю ширину", strip, Act::WSet("bar_strip"), |ui, r| art_bar(ui, r, 0));
    pick(ui, R::new(col(1), y, cw, 160), "плашка", "уже экрана, у края", !strip && !s.bar_float, Act::WSet("bar_panel"), |ui, r| art_bar(ui, r, 1));
    pick(ui, R::new(col(2), y, cw, 160), "плавает", "с отступом и тенью", s.bar_float, Act::WSet("bar_float"), |ui, r| art_bar(ui, r, 2));
    y += 174;
    let half = (b.w - 12) / 2;
    let low = crate::bar::low(s);
    ui.button(R::new(b.x, y, half, 36), "сверху", "", Tone::Plain, !low, true, Act::WSet("bar_top"));
    ui.button(R::new(b.x + half + 12, y, half, 36), "снизу", "", Tone::Plain, low, true, Act::WSet("bar_bottom"));
    y += 50;
    let same = |l: &[&str], c: &[&str], r: &[&str]| s.bar_left == l && s.bar_center == c && s.bar_right == r;
    pick(ui, R::new(col(0), y, cw, 118), "всё", "", same(MODS_FULL.0, MODS_FULL.1, MODS_FULL.2), Act::WSet("mods_full"), |ui, r| art_mods(ui, r, 12));
    pick(ui, R::new(col(1), y, cw, 118), "обычный", "", same(MODS_STD.0, MODS_STD.1, MODS_STD.2), Act::WSet("mods_std"), |ui, r| art_mods(ui, r, 7));
    pick(ui, R::new(col(2), y, cw, 118), "минимум", "", same(MODS_MIN.0, MODS_MIN.1, MODS_MIN.2), Act::WSet("mods_min"), |ui, r| art_mods(ui, r, 3));
}

type Mods = (&'static [&'static str], &'static [&'static str], &'static [&'static str]);
const MODS_FULL: Mods = (&["launcher", "sep", "workspaces", "sep", "mode"], &["tasks"], &["tray", "sep", "net", "kb", "cpu", "ram", "bat", "profile", "sep", "vol", "sep", "clock", "sep", "settings", "power"]);
const MODS_STD: Mods = (&["launcher", "sep", "workspaces"], &["tasks"], &["tray", "sep", "net", "kb", "vol", "bat", "sep", "clock", "power"]);
const MODS_MIN: Mods = (&["workspaces"], &["clock"], &["tray", "kb", "vol", "bat"]);

pub fn dock(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let mut y = b.y;
    ui.toggle(R::new(b.x, y, b.w, 40), "показывать док", "закреплённое и открытые окна", s.dock, Act::SBool("dock"));
    y += 52;
    let cw = (b.w - 24) / 3;
    for (i, (id, name, act)) in [("bottom", "снизу", "dock_bottom"), ("left", "слева", "dock_left"), ("right", "справа", "dock_right")].into_iter().enumerate() {
        pick(ui, R::new(b.x + i as i32 * (cw + 12), y, cw, 176), name, "", s.dock_side == id, Act::WSet(act), |ui, r| art_dock(ui, r, i as u8));
    }
    y += 190;
    let hide = match s.dock_hide.as_str() {
        "never" => "никогда (занимает место)",
        "always" => "всегда — выезжает у края",
        _ => "умно — виден на пустом столе",
    };
    opt_btn(ui, R::new(b.x, y, b.w, 28), "прятать", hide, Act::SCycle("dock_hide"));
    y += 40;
    ui.slider(R::new(b.x, y, b.w, 40), "размер значков", (s.dock_size as f32 - 24.0) / 40.0, &format!("{} px", s.dock_size), 10, Act::SSlider("dock_size", 24.0, 64.0, 10));
}

pub fn windows(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let cw = (b.w - 16) / 2;
    pick(ui, R::new(b.x, b.y, cw, 290), "лента", "окна колонками, лента листается", !s.stacking, Act::WSet("stack_off"), art_tiling);
    pick(ui, R::new(b.x + cw + 16, b.y, cw, 290), "стол", "окна плавают внахлёст, как в DE", s.stacking, Act::WSet("stack_on"), art_float);
    let y = b.y + 310;
    let w3 = (b.w - 24) / 3;
    for (i, (k, label)) in [("win_gap", "между окнами"), ("win_radius", "углы"), ("win_border", "рамка")].into_iter().enumerate() {
        num_btn(ui, R::new(b.x + i as i32 * (w3 + 12), y, w3, 28), label, s.num(k), k);
    }
}

pub fn widgets(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let cw = (b.w - 24) / 3;
    let shown: Vec<&(&str, &str)> = KINDS.iter().filter(|k| k.0 != "script").collect();
    for (i, k) in shown.into_iter().enumerate() {
        let (id, name) = *k;
        let w = s.widgets.iter().find(|w| w.kind == id);
        let act = match w {
            Some(w) => Act::SWidgetDel(w.id),
            None => Act::SWidgetAdd(id),
        };
        let r = R::new(b.x + (i as i32 % 3) * (cw + 12), b.y + (i as i32 / 3) * 186, cw, 174);
        pick(ui, r, name, "", w.is_some(), act, |ui, r| art_widget(ui, r, id));
    }
}

/// главные клавиши столбиками; действия без клавиши не показываются
fn keys_grid(ui: &mut Ui, st: &State, b: R, mut y: i32, ids: &[&str]) -> i32 {
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let shown: Vec<&&str> = ids
        .iter()
        .filter(|id| **id != "files" || st.mods_here.files)
        .filter(|id| **id != "shot" || st.mods_here.shot)
        .filter(|id| crate::keys::BINDS.iter().find(|x| x.id == **id).is_some_and(|bd| !crate::keys::key_of(&st.set, bd).is_empty()))
        .collect();
    for (i, id) in shown.iter().enumerate() {
        let Some(bd) = crate::keys::BINDS.iter().find(|x| x.id == **id) else { continue };
        let x = if i % 2 == 0 { b.x } else { b.x + half + 20 };
        let key = crate::keys::key_of(&st.set, bd).replace("Slash", "/");
        let kw = (ui.w(PX, &key) + 16).max(64);
        ui.cv.pxbox(R::new(x, y, kw, 24), &BoxStyle::new(t.code_bg, t.border));
        ui.cv.text_in(ui.f, PX, R::new(x + 8, y, kw - 12, 24), &key, t.text_hi);
        ui.cv.text_in(ui.f, PX, R::new(x + kw + 10, y, half - kw - 10, 24), crate::keys::title_of(bd), t.text);
        if i % 2 == 1 {
            y += 32;
        }
    }
    if shown.len() % 2 == 1 {
        y += 32;
    }
    y
}

pub fn keys(ui: &mut Ui, st: &State, b: R) {
    let y = crate::keys::layout_rows(ui, st, b, b.y);
    let y = title(ui, b.x, y - 14, b.w, "главные клавиши");
    keys_grid(ui, st, b, y, &["launcher", "settings", "terminal", "files", "clipboard", "shot", "lock", "close"]);
}

pub fn niri(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let Some(w) = &st.welcome else { return };
    let nl = &st.niri_link;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = b.y;
    y += ui.wrap(PX, R::new(b.x, y, b.w, 60), "yami не переписывает конфиг niri: в его начало встаёт одна строка include, и всё, что ниже неё, главнее. Конфига нет — yami положит свой стартовый.", t.text_dim, 3) + 12;
    let ver = match crate::niricfg::version() {
        Some((a, v)) if crate::niricfg::too_old() => (crate::tf!("niri {}.{:02}: слишком старый, нужен 25.11 или новее", "niri {}.{:02}: too old, yami needs 25.11 or newer", "niri {}.{:02}: antigo demais, o yami precisa do 25.11 ou mais novo", a, v), t.warn),
        Some((a, v)) => (format!("niri {a}.{v:02}"), t.text_faint),
        None => ("niri ?".to_string(), t.text_faint),
    };
    let (state, c) = if nl.connected {
        (crate::tf!("подключён: {}", "connected: {}", "conectado: {}", yami_px::paths::pretty(&nl.path)), t.ok)
    } else if nl.exists {
        (crate::tf!("не подключён к {}", "not in {}", "fora de {}", yami_px::paths::pretty(&nl.path)), t.warn)
    } else {
        ("конфига niri пока нет".to_string(), t.warn)
    };
    ui.text_in(PX, R::new(lx, y, b.w - 120, 22), &state, c);
    ui.cv.text_in(ui.f, PX, R::new(b.right() - 110, y, 110, 22), &ver.0, ver.1);
    y += 32;
    if nl.connected {
        ui.button(R::new(lx, y, half, 32), "убрать строку yami", "close", Tone::Plain, false, true, Act::WNiri("disconnect"));
    } else {
        ui.button(R::new(lx, y, half, 32), "подключить одной строкой", "plus", Tone::Primary, false, true, Act::WNiri("connect"));
        if nl.exists && !nl.niri_default {
            ui.button(R::new(rx, y, half, 32), "заменить стартовым yami", "reboot", Tone::Plain, false, true, Act::WNiri("replace"));
        }
    }
    y += 40;
    if nl.exists && !nl.connected && !nl.niri_default {
        y += ui.wrap(PX, R::new(b.x, y, b.w, 20), "заменить: твой файл уйдёт в копию рядом, config.kdl.bak-yami-…", t.text_faint, 1) + 8;
    }
    if !st.niri_note.is_empty() {
        y += ui.wrap(PX, R::new(b.x, y, b.w, 60), &st.niri_note, t.text_dim, 3) + 8;
    }
    y += 6;
    y = title(ui, b.x, y, b.w, "другие программы в конфиге niri");
    if w.others.is_empty() {
        ui.wrap(PX, R::new(b.x, y, b.w, 20), "ничего, что спорило бы с yami: ни второго бара, ни своих уведомлений", t.text_faint, 1);
        return;
    }
    for (i, o) in w.others.iter().enumerate() {
        let what = match o.kind {
            "bar" => "второй бар",
            "shell" => "второй шелл",
            "notif" => "свои уведомления",
            "wall" => "свои обои",
            "idle" => "своё ожидание",
            _ => "",
        };
        ui.cv.text_in(ui.f, PX, R::new(lx, y, 140, 26), o.prog, t.text_hi);
        ui.text_in(PX, R::new(lx + 140, y, b.w - 140 - 150, 26), what, t.text_dim);
        ui.button(R::new(b.right() - 140, y, 140, 26), "не запускать", "stop", Tone::Plain, false, true, Act::WOther(i));
        y += 32;
    }
    ui.wrap(PX, R::new(b.x, y + 4, b.w, 40), "строка в конфиге станет комментарием с пометкой yami, сам файл — копией рядом", t.text_faint, 2);
}

pub fn modules(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let Some(w) = &st.welcome else { return };
    let half = (b.w - 20) / 2;
    let mut y = b.y;
    // поставлен пакетом: выбирать здесь нечего, модули ставит менеджер пакетов
    let packaged = !crate::update::enabled();
    let hint = if packaged { "Стоит то, что поставлено пакетами." } else { "Отмеченные поставятся, снятые уйдут." };
    ui.text_in(PX, R::new(b.x, y, b.w, 20), hint, t.text_dim);
    y += 30;
    for (i, m) in w.mods.iter().enumerate() {
        let x = if i % 2 == 0 { b.x } else { b.x + half + 20 };
        let r = R::new(x, y, half, 40);
        if packaged {
            ui.cv.text_in(ui.f, PX, R::new(r.x, r.y, r.w, 20), m.name, if m.have { t.text_hi } else { t.text_dim });
            ui.text_in(PX, R::new(r.x, r.y + 18, r.w, 20), m.what, t.text_faint);
            if m.have {
                let nw = ui.w(PX, m.name);
                ui.text_in(PX, R::new(r.x + nw + 10, r.y, r.w - nw - 10, 20), "стоит", t.ok);
            }
        } else {
            ui.toggle(r, m.name, m.what, m.want, Act::WMod(i));
            let nw = ui.w(PX, m.name);
            let (tag, c) = match (m.have, m.want) {
                (true, true) => ("стоит", t.ok),
                (false, true) => ("поставится", t.red),
                (true, false) => ("уйдёт", t.warn),
                _ => ("", t.text_faint),
            };
            if !tag.is_empty() {
                ui.text_in(PX, R::new(r.x + 26 + nw + 10, r.y + 2, r.w - 36 - nw, 20), tag, c);
            }
        }
        if i % 2 == 1 {
            y += 48;
        }
    }
    if w.mods.len() % 2 == 1 {
        y += 48;
    }
    y += 4;
    install_row(ui, st, w, b, y, half, packaged);
}

/// под модулями: что с установкой сейчас и кнопка
fn install_row(ui: &mut Ui, st: &State, w: &Welcome, b: R, mut y: i32, half: i32, packaged: bool) {
    let t = ui.t;
    if packaged {
        ui.wrap(PX, R::new(b.x, y, b.w, 60), "yami поставлен пакетом: модули ставит менеджер пакетов — пакеты yami-files, yami-view, yami-play, yami-shot, yami-agents, yami-greet", t.text_dim, 3);
        return;
    }
    if !crate::wizinst::can_install() {
        ui.wrap(PX, R::new(b.x, y, b.w, 60), "ставить нечем: нет ни исходников yami, ни git. Каждый модуль ставится и сам: yami-files install", t.warn, 3);
        return;
    }
    let inst = &w.inst;
    if inst.running {
        let (k, n, text) = inst.step.clone().unwrap_or((0, 1, String::new()));
        let cells = 24;
        let cw = (b.w - (cells - 1) * 3) / cells;
        let lit = (k as i32 * cells) / (n.max(1) as i32);
        for i in 0..cells {
            ui.cv.fill(R::new(b.x + i * (cw + 3), y, cw, 10), if i < lit { t.red } else { t.border });
        }
        y += 18;
        ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w, 20), &format!("[{k}/{n}] {text}"), t.text);
        y += 24;
        for l in inst.lines.iter().rev().take(3).rev() {
            ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w, 18), l, t.text_faint);
            y += 18;
        }
        return;
    }
    if let Some(e) = &inst.failed {
        y += ui.wrap(PX, R::new(b.x, y, b.w, 60), e, t.warn, 3) + 8;
    } else if inst.done && !w.changed() {
        ui.text_in(PX, R::new(b.x, y, b.w, 22), "готово: модули на месте", t.ok);
        return;
    }
    if !w.changed() {
        ui.wrap(PX, R::new(b.x, y, b.w, 40), "отметь, что поставить или убрать; ход установки виден здесь", t.text_faint, 2);
        return;
    }
    let go = if inst.failed.is_some() { "ещё раз" } else { "применить" };
    if inst.pass {
        let f = st.field_focus == Some("w_pass");
        ui.field(R::new(b.x, y, half, 30), &w.pass, if inst.wrong { "неверный пароль, ещё раз" } else { "пароль sudo — для пакетов" }, f, true, Act::SField("w_pass"));
        ui.button(R::new(b.x + half + 20, y, half, 30), go, "down", Tone::Primary, false, !w.pass.text.is_empty(), Act::WInstall);
        y += 38;
        ui.wrap(PX, R::new(b.x, y, b.w, 40), "пароль уходит только sudo и нигде не сохраняется", t.text_faint, 2);
    } else {
        ui.button(R::new(b.x, y, half, 30), go, "down", Tone::Primary, false, true, Act::WInstall);
        if !inst.sudo {
            ui.wrap(PX, R::new(b.x + half + 20, y, half, 40), "sudo нет: пакеты потом одной командой — yami update need", t.text_faint, 2);
        }
    }
}

pub fn services(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let w = b.w;
    let mut y = b.y;
    ui.toggle(R::new(b.x, y, w, 40), "уведомления yami", "выкл — mako, dunst и т.п. (после перезапуска шелла)", s.agent_notif, Act::SBool("agent_notif"));
    y += 50;
    ui.toggle(R::new(b.x, y, w, 40), "трей", "значки программ в баре", s.agent_tray, Act::SBool("agent_tray"));
    y += 50;
    if st.mods_here.agents {
        let ag = crate::agents::AgentsCfg::load();
        ui.toggle(R::new(b.x, y, w, 40), "подключать флешки и телефоны сами", "флешки, диски, телефоны, камеры", ag.automount, Act::SBool("automount"));
        y += 50;
    }
    ui.toggle(R::new(b.x, y, w, 40), "обложки из интернета", "выкл — только то, что отдаёт плеер", s.media_art_net, Act::SBool("media_art_net"));
    y += 50;
    ui.toggle(R::new(b.x, y, w, 40), "тексты песен из интернета", "выкл — только плеер и файлы .lrc", s.lyrics_net, Act::SBool("lyrics_net"));
}

pub fn done(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let y = title(ui, b.x, b.y + 6, b.w, "главные клавиши");
    let y = keys_grid(ui, st, b, y, &["launcher", "settings", "terminal", "files", "clipboard", "shot", "walls", "lock", "overview", "hotkeys", "close", "power"]);
    ui.wrap(PX, R::new(b.x, y + 6, b.w, 40), "все клавиши — в настройках → клавиши; их подсказка в niri — Mod+Shift+/", t.text_faint, 2);
}

/// выборы мастера, у которых в настройках нет своего действия «поставить именно это»
pub fn set(app: &mut App, k: &str) {
    if let Some(l) = k.strip_prefix("level").and_then(|n| n.parse::<usize>().ok()) {
        app.set_pixel_level(l);
        app.look_apply();
        return;
    }
    if let Some(side) = k.strip_prefix("dock_") {
        // круговое действие настроек: ему же и пересоздавать панель на новом краю
        for _ in 0..4 {
            if app.st.set.dock_side == side {
                break;
            }
            app.settings_click(&Act::SCycle("dock_side"));
        }
        return;
    }
    if matches!(k, "stack_on" | "stack_off") {
        if app.st.set.stacking != (k == "stack_on") {
            app.settings_click(&Act::SBool("stacking"));
        }
        return;
    }
    let s = &mut app.st.set;
    let list = |m: &[&str]| m.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let mods = |s: &mut crate::config::Settings, m: Mods| {
        s.bar_left = list(m.0);
        s.bar_center = list(m.1);
        s.bar_right = list(m.2);
    };
    match k {
        "bar_strip" => {
            s.bar_float = false;
            s.bar_width_pc = 100;
        }
        "bar_panel" => {
            s.bar_float = false;
            s.bar_width_pc = 70;
        }
        "bar_float" => {
            s.bar_float = true;
            s.bar_width_pc = 100;
        }
        "bar_top" => s.bar_pos = "top".into(),
        "bar_bottom" => s.bar_pos = "bottom".into(),
        "mods_full" => mods(s, MODS_FULL),
        "mods_std" => mods(s, MODS_STD),
        "mods_min" => mods(s, MODS_MIN),
        _ => return,
    }
    app.settings_changed();
}
