// Панель управления (из yami-lite): ПКМ по пустому месту бара или `yami msg control`. Слева — часы и
// дата на обоях под вуалью, время работы системы (щелчок — выбор обоев); справа — два вертикальных
// ползунка клетками: громкость и яркость (подсветка ноутбука, bright.rs; без неё ползунок погашен).
// Внизу — плитки на выбор (controlTiles, до MAX штук): каждая — уже существующее действие шелла
// (Wi-Fi, bluetooth, не беспокоить, ночной свет…), своё здесь только рисование.

use crate::app::{App, SKind, State};
use crate::clock::{local_now, RU_DAY_FULL, RU_MON};
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Ui};

/// плитки: ключ, подпись (подсказка при наведении), значок
pub const TILES: &[(&str, &str, &str)] = &[
    ("wifi", "Wi-Fi", "wifi"),
    ("bt", "Bluetooth", "bt"),
    ("dnd", "не беспокоить", "bellx"),
    ("night", "ночной свет", "moon"),
    ("mic", "микрофон", "mic"),
    ("mute", "звук", "speaker"),
    ("theme", "светлая тема", "sun"),
    ("curtain", "шторка", "eye"),
    ("settings", "настройки", "gear"),
    ("lock", "заблокировать", "lock"),
    ("power", "питание", "power"),
];
/// плиток в ряду — сколько квадратов влезает
pub const MAX: usize = 6;
pub const SIZE: (i32, i32) = (480, 300);

/// плитка горит
fn lit(st: &State, k: &str) -> bool {
    match k {
        "wifi" => st.net.wifi_present && st.net.wifi_on,
        "bt" => st.bt.powered,
        "dnd" => st.set.dnd,
        "night" => st.night.on,
        "mic" => !st.audio.mic_muted,
        "mute" => !st.audio.muted,
        "theme" => st.set.light,
        "curtain" => st.curtain.is_some(),
        _ => false,
    }
}

/// что делает плитка: действие шелла, которое уже есть у его панелей
fn act_of(st: &State, k: &str) -> Act {
    match k {
        "wifi" => Act::Wifi(!st.net.wifi_on),
        "bt" => Act::BtPower(!st.bt.powered),
        "dnd" => Act::Dnd,
        "night" => Act::NightToggle,
        "mic" => Act::MicMute,
        "mute" => Act::VolMute,
        "theme" => Act::SCycle(if st.set.light { "theme_dark" } else { "theme_light" }),
        "curtain" => Act::Curtain,
        "settings" => Act::Settings,
        "lock" => Act::LockNow,
        "power" => Act::Power,
        _ => Act::Block,
    }
}

/// значок по состоянию: колокольчик и звук — перечёркнуты
fn icon_of(st: &State, k: &str, icon: &'static str) -> &'static str {
    match k {
        "dnd" if !st.set.dnd => "bell",
        "mute" if st.audio.muted => "mute",
        _ => icon,
    }
}

/// время работы системы из /proc/uptime (читается при отрисовке панели — она открыта недолго)
fn uptime() -> String {
    let s = std::fs::read_to_string("/proc/uptime").ok().and_then(|t| t.split_whitespace().next()?.parse::<f64>().ok()).unwrap_or(0.0) as u64;
    let (d, h, m) = (s / 86400, s / 3600 % 24, s / 60 % 60);
    if d > 0 {
        crate::tf!("работает {} д {} ч {} мин", "up {} d {} h {} min", "ligado há {} d {} h {} min", d, h, m)
    } else {
        crate::tf!("работает {} ч {} мин", "up {} h {} min", "ligado há {} h {} min", h, m)
    }
}

/// вертикальный ползунок: клетки снизу вверх, значок и процент под ними; off — погашен
fn vslider(ui: &mut Ui, r: R, icon: &str, v: f32, off: bool, act: Act) {
    let t = ui.t;
    let hov = !off && ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, if hov { t.red } else { t.border }));
    let track = R::new(r.x + 12, r.y + 10, r.w - 24, r.h - 62);
    let cells = 12;
    let ch = (track.h - (cells - 1) * 2) / cells;
    let on = if off { 0 } else { (v.clamp(0.0, 1.0) * cells as f32).round() as i32 };
    for i in 0..cells {
        let y = track.y + track.h - (i + 1) * ch - i * 2;
        ui.cv.fill(R::new(track.x, y, track.w, ch), if i < on { t.red } else { t.surface_hi.lerp(t.border, 0.45) });
    }
    ui.cv.icon(icon, r.x + r.w / 2 - 7, r.bottom() - 46, 2, if off { t.text_faint } else { t.red });
    let pct = if off { "—".to_string() } else { format!("{}%", (v * 100.0).round()) };
    ui.text_center(PX, R::new(r.x, r.bottom() - 26, r.w, 20), &pct, if off { t.text_faint } else { t.text });
    if !off {
        ui.zone(R::new(r.x, track.y - 4, r.w, track.h + 8), act, Cur::Pointer);
    }
}

pub fn draw(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    const GAP: i32 = 10;
    let side = (b.w - (MAX as i32 - 1) * GAP) / MAX as i32;
    let top_h = b.h - side - 12;
    let sw = 60;
    // часы на обоях: щелчок — выбор обоев
    let card = R::new(b.x, b.y, b.w - 2 * sw - GAP - 12, top_h);
    ui.cv.pxbox(card, &BoxStyle::new(t.code_bg, t.border));
    let inner = card.inset(2);
    ui.cv.push_clip(inner);
    if let Some(img) = &st.wall {
        let k = (inner.w as f32 / img.w.max(1) as f32).max(inner.h as f32 / img.h.max(1) as f32);
        let (dw, dh) = ((img.w as f32 * k).ceil() as i32, (img.h as f32 * k).ceil() as i32);
        ui.cv.image_scaled(img, R::new(inner.x + (inner.w - dw) / 2, inner.y + (inner.h - dh) / 2, dw, dh), 1.0);
    }
    ui.cv.fill(inner, yami_px::face::veil(&t, Some(0.45)));
    ui.cv.pop_clip();
    if ui.hover(card) {
        ui.cv.icon("image", card.right() - 26, card.y + 10, 2, t.text);
    }
    ui.zone(card, Act::SWall("browse"), Cur::Pointer);
    let tm = local_now();
    let face = font::px(48);
    let time = if st.set.clock_seconds { format!("{:02}:{:02}:{:02}", tm.hour, tm.min, tm.sec) } else { format!("{:02}:{:02}", tm.hour, tm.min) };
    let (lx, ty) = (card.x + 16, card.y + 14);
    ui.text(face, lx, ty, &time, t.text_hi);
    let date = crate::tf!("{}, {} {}", "{}, {} {}", "{}, {} de {}", crate::i18n::tr(RU_DAY_FULL[tm.wday as usize]), tm.day, crate::i18n::tr(RU_MON[(tm.mon - 1) as usize]));
    let ly = ty + ui.f.line(face) + 4;
    ui.cv.text_in(ui.f, PX, R::new(lx, ly, card.w - 32, 18), &date, t.text);
    // низ карточки: время работы, а под мышью на плитке — её название (у плиток одни значки)
    let tiles: Vec<&(&str, &str, &str)> = st.set.control_tiles.iter().filter_map(|k| TILES.iter().find(|x| x.0 == k)).take(MAX).collect();
    let n = tiles.len() as i32;
    let x0 = b.x + (b.w - n * side - (n - 1).max(0) * GAP) / 2;
    let ty = b.bottom() - side;
    let at = |i: usize| R::new(x0 + i as i32 * (side + GAP), ty, side, side);
    match (0..tiles.len()).find(|i| ui.hover(at(*i))) {
        Some(i) => ui.text_in(PX, R::new(lx, card.bottom() - 30, card.w - 32, 18), tiles[i].1, t.text),
        None => ui.cv.text_in(ui.f, PX, R::new(lx, card.bottom() - 30, card.w - 32, 18), &uptime(), t.text_dim.lerp(t.text, 0.5)),
    };
    // ползунки во всю высоту часов
    let sx = card.right() + 12;
    let a = &st.audio;
    vslider(ui, R::new(sx, b.y, sw, top_h), if a.muted { "mute" } else { "speaker" }, a.volume as f32, a.muted, Act::CtlVol);
    vslider(ui, R::new(sx + sw + GAP, b.y, sw, top_h), "sun", st.bright.unwrap_or(0.0), st.bright.is_none(), Act::CtlBright);
    // плитки: квадраты по центру ряда, одни значки
    for (i, (k, _, icon)) in tiles.into_iter().enumerate() {
        let rr = at(i);
        let on = lit(st, k);
        let hov = ui.hover(rr);
        ui.cv.pxbox(rr, &BoxStyle::new(if on { t.red } else if hov { t.surface_hi } else { t.surface }, if on { t.ember } else if hov { t.red } else { t.border }).bevel(true));
        ui.cv.icon(icon_of(st, k, icon), rr.x + (rr.w - 28) / 2, rr.y + (rr.h - 28) / 2, 4, if on { t.on_accent } else { t.text });
        ui.zone(rr, act_of(st, k), Cur::Pointer);
    }
}

impl App {
    /// ползунки панели: громкость — системе сразу (pactl), яркость — через logind (bright::set);
    /// на экране — тут же, не дожидаясь их событий
    pub fn control_drag(&mut self, act: &Act, frac: f32) {
        match act {
            Act::CtlVol => {
                self.st.audio.volume = frac as f64;
                crate::audio::set_volume(frac as f64);
            }
            Act::CtlBright => {
                self.st.bright = Some(frac.max(0.01));
                crate::bright::set(frac);
            }
            _ => return,
        }
        self.dirty(&SKind::Popup);
    }

    /// ПКМ по пустому месту бара, `msg control`: открыть панель (открыта — закрыть)
    pub fn toggle_control(&mut self, out: &str, ax: i32) {
        if self.st.popup_is("control") {
            self.close_popup();
        } else {
            self.open_popup("control", out, ax);
        }
    }
}
