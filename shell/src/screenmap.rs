// Схема экрана для выбора места щелчком: монитор в масштабе, бар у своего края, а места, куда можно
// поставить бар (края), карточки уведомлений и OSD (углы, середины), — кнопками прямо на схеме.
// Настройки → бар → вид, настройки → уведомления, шаг первой настройки и панель управления.

use crate::app::State;
use crate::gfx::{BoxStyle, R};
use crate::ui::{Act, Cur, Ui};

/// что выбирается на схеме
#[derive(Clone, Copy, PartialEq)]
pub enum Pick {
    /// край бара (bar_pos)
    Bar,
    /// угол карточек уведомлений (notif_pos)
    Notif,
    /// место OSD громкости и яркости (osd_pos)
    Osd,
}

/// экран 16:9 по ширине r (высота — сколько выйдет, не больше r.h); возвращает его прямоугольник
fn screen(r: R) -> R {
    let h = (r.w * 9 / 16).min(r.h);
    let w = h * 16 / 9;
    R::new(r.x + (r.w - w) / 2, r.y, w, h)
}

/// бар на схеме: настоящая форма (bar::geo) в масштабе 1920×1080
fn bar_on(s: &crate::config::Settings, scr: R) -> R {
    let v = crate::bar::vert(s);
    let k = scr.w as f32 / 1920.0;
    let sc = |x: i32| (x as f32 * k).round() as i32;
    let (m, side, _) = crate::bar::geo(s, if v { 1080 } else { 1920 });
    // толщина — заметнее масштаба: на схеме в 300 px настоящие 32 px были бы ниткой
    let th = (sc(crate::app::BAR_H) * 2).clamp(7, 14);
    let (m, side) = (sc(m) * 2, sc(side));
    match s.bar_pos.as_str() {
        "bottom" => R::new(scr.x + side, scr.bottom() - m - th, scr.w - 2 * side, th),
        "left" => R::new(scr.x + m, scr.y + side, th, scr.h - 2 * side),
        "right" => R::new(scr.right() - m - th, scr.y + side, th, scr.h - 2 * side),
        _ => R::new(scr.x + side, scr.y + m, scr.w - 2 * side, th),
    }
}

/// место на экране без бара: туда встают карточки и OSD
fn free(s: &crate::config::Settings, scr: R, bar: R) -> R {
    let gap = 3;
    match s.bar_pos.as_str() {
        "bottom" => R::new(scr.x, scr.y, scr.w, bar.y - scr.y - gap),
        "left" => R::new(bar.right() + gap, scr.y, scr.right() - bar.right() - gap, scr.h),
        "right" => R::new(scr.x, scr.y, bar.x - scr.x - gap, scr.h),
        _ => R::new(scr.x, bar.bottom() + gap, scr.w, scr.bottom() - bar.bottom() - gap),
    }
}

/// прямоугольник места `k` размером w×h внутри f (углы, середины сторон, центр), отступ p
fn spot(k: &str, f: R, w: i32, h: i32, p: i32) -> R {
    let x = if k.ends_with("left") || k == "left" {
        f.x + p
    } else if k.ends_with("right") || k == "right" {
        f.right() - p - w
    } else {
        f.x + (f.w - w) / 2
    };
    let y = if k.starts_with("top") {
        f.y + p
    } else if k.starts_with("bottom") {
        f.bottom() - p - h
    } else {
        f.y + (f.h - h) / 2
    };
    R::new(x, y, w, h)
}

/// схема с выбором; возвращает высоту
pub fn draw(ui: &mut Ui, st: &State, r: R, what: Pick) -> i32 {
    let t = ui.t;
    let s = &st.set;
    let scr = screen(r);
    // рамка монитора с подставкой: схема читается как экран, а не как ещё одна кнопка
    ui.cv.pxbox(R::new(scr.x - 4, scr.y - 4, scr.w + 8, scr.h + 8), &BoxStyle::new(t.code_bg, t.border));
    ui.cv.fill(R::new(scr.x + scr.w / 2 - 10, scr.bottom() + 4, 20, 4), t.border);
    ui.cv.fill(R::new(scr.x + scr.w / 2 - 22, scr.bottom() + 8, 44, 2), t.border);
    ui.cv.fill(scr, t.bg);
    // обои — клеточки палитры, как в предпросмотрах мастера
    for (i, y) in (scr.y + 2..scr.bottom() - 2).step_by(6).enumerate() {
        for (j, x) in (scr.x + 2..scr.right() - 2).step_by(6).enumerate() {
            if (i * 7 + j * 3) % 11 == 0 {
                ui.cv.fill(R::new(x, y, 2, 2), t.border.fade(0.5));
            }
        }
    }
    let bar = bar_on(s, scr);
    let f = free(s, scr, bar);
    match what {
        Pick::Bar => {
            // свободные края — полупрозрачные полосы под мышью, текущий — сам бар
            let th = bar.w.min(bar.h);
            for (k, _) in crate::bar::EDGES {
                let er = match *k {
                    "bottom" => R::new(scr.x + th + 2, scr.bottom() - th - 2, scr.w - 2 * th - 4, th),
                    "left" => R::new(scr.x + 2, scr.y + th + 2, th, scr.h - 2 * th - 4),
                    "right" => R::new(scr.right() - th - 2, scr.y + th + 2, th, scr.h - 2 * th - 4),
                    _ => R::new(scr.x + th + 2, scr.y + 2, scr.w - 2 * th - 4, th),
                };
                let cur = s.bar_pos == *k;
                // зона шире полосы: в край схемы легко попасть
                let hit = match *k {
                    "bottom" => R::new(er.x, er.y - 8, er.w, er.h + 10),
                    "left" => R::new(er.x - 2, er.y, er.w + 10, er.h),
                    "right" => R::new(er.x - 8, er.y, er.w + 10, er.h),
                    _ => R::new(er.x, er.y - 2, er.w, er.h + 10),
                };
                if !cur && ui.hover(hit) {
                    ui.cv.fill(er, t.red.fade(0.45));
                    ui.cv.outline(er, t.red, 1);
                } else if !cur {
                    // свободный край — пунктиром: сюда можно
                    let c = t.text_faint.fade(0.6);
                    if er.w > er.h {
                        for x in (er.x..er.right()).step_by(4) {
                            ui.cv.fill(R::new(x, er.cy(), 2, 1), c);
                        }
                    } else {
                        for y in (er.y..er.bottom()).step_by(4) {
                            ui.cv.fill(R::new(er.x + er.w / 2, y, 1, 2), c);
                        }
                    }
                }
                if !cur {
                    ui.zone(hit, Act::SPlace("bar_pos", k), Cur::Pointer);
                }
            }
            bar_shape(ui, st, bar);
        }
        Pick::Notif | Pick::Osd => {
            bar_shape(ui, st, bar);
            let (key, list, cur) = if what == Pick::Notif { ("notif_pos", crate::notif::POS, &s.notif_pos) } else { ("osd_pos", crate::notif::OSD_POS, &s.osd_pos) };
            // карточки — стопкой из трёх, OSD — одна широкая плашка
            let (sw, sh) = if what == Pick::Notif { (f.w / 5, f.h / 4) } else { (f.w / 4, (f.h / 9).max(8)) };
            for (k, _) in list {
                let sp = spot(k, f, sw, sh, 6);
                let on = cur == k;
                let hov = ui.hover(sp);
                if what == Pick::Notif {
                    let ch = (sh - 4) / 3;
                    for i in 0..3 {
                        let rr = R::new(sp.x, sp.y + i * (ch + 2), sp.w, ch);
                        let fill = if on { if i == 0 { t.red } else { t.red.fade(0.6 - i as f32 * 0.15) } } else if hov { t.surface_hi } else { t.surface.fade(0.8) };
                        ui.cv.fill(rr, fill);
                        if !on {
                            ui.cv.outline(rr, if hov { t.red } else { t.border.fade(0.7) }, 1);
                        }
                    }
                } else {
                    ui.cv.pxbox(sp, &BoxStyle::new(if on { t.red } else if hov { t.surface_hi } else { t.surface.fade(0.8) }, if on { t.ember } else if hov { t.red } else { t.border.fade(0.7) }));
                    // полоска громкости внутри
                    let gw = (sp.w - 8) * 3 / 5;
                    ui.cv.fill(R::new(sp.x + 4, sp.cy(), gw, 2), if on { t.on_accent } else { t.text_faint });
                }
                if !on {
                    ui.zone(sp, Act::SPlace(key, k), Cur::Pointer);
                }
            }
        }
    }
    scr.h + 12
}

/// бар на схеме: полоса или плашка с тенью, на ней модули клетками — у начала акцентная, в
/// середине часы, у конца тусклые
fn bar_shape(ui: &mut Ui, st: &State, bar: R) {
    let t = ui.t;
    if crate::bar::strip(&st.set) {
        ui.cv.fill(bar, t.surface);
        ui.cv.outline(bar, t.border, 1);
    } else {
        ui.cv.pxbox(bar, &BoxStyle::new(t.surface, t.border).shadow(1));
    }
    let th = bar.w.min(bar.h);
    let c = (th / 3).max(2);
    let len = bar.w.max(bar.h);
    let along = |ui: &mut Ui, at: i32, n: i32, col: crate::gfx::C| {
        let off = (th - c) / 2;
        let r = if bar.w >= bar.h { R::new(bar.x + at, bar.y + off, n, c) } else { R::new(bar.x + off, bar.y + at, c, n) };
        ui.cv.fill(r, col);
    };
    for i in 0..3 {
        along(ui, 3 + i * (c + 2), c, if i == 0 { t.red } else { t.dim });
    }
    if !st.set.bar_center.is_empty() {
        along(ui, len / 2 - c * 2, c * 4, t.text);
    }
    for i in 0..4 {
        along(ui, len - 3 - c - i * (c + 2), c, t.text_faint);
    }
}
