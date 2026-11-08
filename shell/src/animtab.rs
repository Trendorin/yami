// Настройки → анимации: общие (вкл, закрытие, скорость, узор, клетки) и своя анимация у каждой
// группы панелей (config::ANIM_GROUPS → `anims` в shell.json): узор появления и закрытия,
// длительность, движение. Рядом с группой — образец: щелчок по нему (или любая перемена) проигрывает
// открытие и закрытие так, как их нарисует шелл (render.rs `reveal_of`, notif.rs).

use crate::app::{App, State};
use crate::config::{Anim, ANIM_GROUPS, ANIM_SPEEDS, REVEALS};
use crate::font::PX;
use crate::gfx::{BoxStyle, R};
use crate::settabs::reveal_name;
use crate::settings::{num_btn, opt_btn, title};
use crate::ui::{Act, Cur, Ui};
use std::time::Instant;

/// что меняет щелчок в строке группы
const OPEN: u8 = 0;
const CLOSE: u8 = 1;
const SPEED: u8 = 2;
const MOTION: u8 = 3;
const DEMO: u8 = 4;
/// пауза между открытием и закрытием образца, мс
const HOLD: f32 = 600.0;

fn style_name(id: &str) -> &'static str {
    if id.is_empty() {
        "как везде"
    } else {
        reveal_name(id)
    }
}

/// образец сейчас: проявленность и закрывается ли; None — стоит открытым
fn demo_k(st: &State, gi: usize) -> Option<(f32, bool)> {
    let (g, at) = st.anim_demo?;
    if g as usize != gi || !st.set.anim() {
        return None;
    }
    let id = ANIM_GROUPS[gi].0;
    let s = &st.set;
    let open = if s.reveal_for(id, false) == "none" { 0.0 } else { s.anim_ms(id) };
    let close = if !s.anim_close || s.reveal_for(id, true) == "none" { 0.0 } else { s.anim_ms(id) * 0.7 };
    let el = at.elapsed().as_secs_f32() * 1000.0;
    if el < open {
        let t = el / open;
        return Some((1.0 - (1.0 - t).powi(3), false));
    }
    if el < open + HOLD {
        return Some((1.0, false));
    }
    let el = el - open - HOLD;
    if el < close {
        let t = el / close;
        return Some((1.0 - t * t, true));
    }
    (el < close + 300.0).then_some((0.0, true))
}

/// образец группы: «сцена» и на ней панель той формы, что у группы
fn sample(ui: &mut Ui, st: &State, gi: usize, r: R) {
    let t = ui.t;
    let id = ANIM_GROUPS[gi].0;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(t.bg, if hov { t.red } else { t.border }));
    let (k, closing) = demo_k(st, gi).unwrap_or((1.0, false));
    if st.anim_demo.is_some_and(|d| d.0 as usize == gi) {
        ui.anim = true;
    }
    let a = st.set.anim_of(id);
    let style = st.set.reveal_for(id, closing);
    // место панели на сцене, как у настоящей: окно в центре, всплывашка под баром, карточка справа…
    let p = match id {
        "popups" => R::new(r.x + 44, r.y + 13, 44, 38),
        "menus" => R::new(r.x + 30, r.y + 16, 40, 34),
        "notifs" => R::new(r.x + 42, r.y + 8, 48, 24),
        "widgets" => R::new(r.x + 10, r.y + 14, 34, 36),
        "dock" => R::new(r.x + 20, r.y + r.h - 18, r.w - 40, 12),
        _ => R::new(r.x + 16, r.y + 12, r.w - 32, r.h - 24),
    };
    if id == "popups" {
        ui.cv.fill(R::new(r.x + 3, r.y + 3, r.w - 6, 6), t.surface_hi);
    }
    // движение: окна и всплывашки опускаются на место, карточки въезжают справа, док — снизу
    let off = ((1.0 - k) * 8.0).round() as i32;
    let p = if !a.motion {
        p
    } else {
        match id {
            "notifs" => R::new(p.x + off * 3, p.y, p.w, p.h),
            "dock" => R::new(p.x, p.y + off, p.w, p.h),
            _ => R::new(p.x, p.y - off, p.w, p.h),
        }
    };
    if k <= 0.0 {
        ui.zone(r, Act::SAnim(gi as u8, DEMO), Cur::Pointer);
        return;
    }
    let was = ui.cv.opacity;
    let pixel = !matches!(style, "fade" | "none");
    ui.cv.opacity = was * if pixel { (0.35 + 0.65 * k).min(1.0) } else { k };
    // рисовать только внутри сцены: въезд справа не вылезает за рамку
    ui.cv.push_clip(R::new(r.x + 2, r.y + 2, r.w - 4, r.h - 4));
    ui.cv.pxbox(p, &BoxStyle::new(t.surface, t.border).shadow(2));
    let rows = ((p.h - 8) / 8).clamp(1, 4);
    for i in 0..rows {
        let w = (p.w - 12 - i * 6).max(6);
        ui.cv.fill(R::new(p.x + 6, p.y + 6 + i * 8, w, 3), if i == 0 { t.red } else { t.text_faint });
    }
    if pixel && k < 1.0 {
        ui.cv.dissolve_rect_as(R::new(p.x - 3, p.y - 3, p.w + 8, p.h + 8), k, 2, yami_px::gfx::Reveal::from_id(style), st.set.reveal_grain);
    }
    ui.cv.pop_clip();
    ui.cv.opacity = was;
    ui.zone(r, Act::SAnim(gi as u8, DEMO), Cur::Pointer);
}

pub fn tab(ui: &mut Ui, st: &State, b: R) {
    let s = &st.set;
    let t = ui.t;
    let half = (b.w - 20) / 2;
    let x2 = b.x + half + 20;
    let mut y = title(ui, b.x, b.y, b.w, "везде");
    ui.toggle(R::new(b.x, y, half, 26), "анимации", "", s.animations, Act::SBool("animations"));
    ui.toggle(R::new(x2, y, half, 26), "анимация закрытия", "", s.anim_close, Act::SBool("anim_close"));
    y += 34;
    ui.slider(R::new(b.x, y, b.w, 40), "скорость анимаций (меньше — быстрее)", (s.anim_speed - 0.3) / 1.7, &format!("×{:.1}", s.anim_speed), 17, Act::SSlider("anim_speed", 0.3, 2.0, 17));
    y += 50;
    // общий узор: у групп «как везде» и во всех программах yami
    opt_btn(ui, R::new(b.x, y, half, 26), "узор", reveal_name(&s.reveal), Act::SCycle("reveal"));
    num_btn(ui, R::new(x2, y, half, 26), "размер клеток", s.num("reveal_grain"), "reveal_grain");
    y += 32;
    if s.lite {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "лёгкий режим: анимаций нет (настройки → вид)", t.warn);
        y += 22;
    } else if !s.animations {
        ui.text_in(PX, R::new(b.x, y, b.w, 20), "анимации выключены: всё открывается сразу", t.text_faint);
        y += 22;
    }
    y += 8;
    y = title(ui, b.x, y, b.w, "по панелям");
    ui.text_in(PX, R::new(b.x, y, b.w, 20), "щелчок по образцу покажет, как это выглядит", t.text_faint);
    y += 26;
    let sw = 96;
    let cx = b.x + sw + 14;
    let cw = (b.w - sw - 14 - 12) / 2;
    for (gi, (id, name, what, _)) in ANIM_GROUPS.iter().enumerate() {
        let a = s.anim_of(id);
        sample(ui, st, gi, R::new(b.x, y, sw, 76));
        let nw = ui.text_in(PX, R::new(cx, y, cw, 18), name, t.text);
        ui.text_in(PX, R::new(cx + nw + 10, y, b.right() - cx - nw - 10, 18), what, t.text_faint);
        let g = gi as u8;
        opt_btn(ui, R::new(cx, y + 22, cw, 26), "появление", style_name(&a.open), Act::SAnim(g, OPEN));
        opt_btn(ui, R::new(cx + cw + 12, y + 22, cw, 26), "закрытие", style_name(&a.close), Act::SAnim(g, CLOSE));
        let ms = s.anim_ms(id).round() as i32;
        opt_btn(ui, R::new(cx, y + 52, cw, 26), "длительность", &crate::tf!("{} мс", "{} ms", "{} ms", ms), Act::SAnim(g, SPEED));
        ui.toggle(R::new(cx + cw + 12, y + 52, cw, 26), "движение", "", a.motion, Act::SAnim(g, MOTION));
        y += 92;
    }
    // высота вкладки — по нижней зоне (Canvas::reach)
    let _ = y;
}

impl App {
    /// щелчок вкладки анимаций; true — он был её
    pub fn anim_click(&mut self, act: &Act) -> bool {
        let &Act::SAnim(g, what) = act else { return false };
        let Some(&(id, ..)) = ANIM_GROUPS.get(g as usize) else { return true };
        if what != DEMO {
            let a = self.st.set.anims.entry(id.to_string()).or_default();
            let next = |cur: &str| -> String {
                // как везде → узоры по кругу → как везде
                let i = REVEALS.iter().position(|x| *x == cur);
                match i {
                    None => REVEALS[0].to_string(),
                    Some(i) if i + 1 < REVEALS.len() => REVEALS[i + 1].to_string(),
                    _ => String::new(),
                }
            };
            match what {
                OPEN => a.open = next(&a.open),
                CLOSE => a.close = next(&a.close),
                SPEED => {
                    let i = ANIM_SPEEDS.iter().position(|x| (x - a.speed).abs() < 0.01).map_or(2, |i| (i + 1) % ANIM_SPEEDS.len());
                    a.speed = ANIM_SPEEDS[i];
                }
                MOTION => a.motion = !a.motion,
                _ => {}
            }
            // всё как по умолчанию — ключ не нужен, файл настроек остаётся коротким
            if *a == Anim::default() {
                self.st.set.anims.remove(id);
            }
            self.settings_changed();
        }
        self.st.anim_demo = Some((g, Instant::now()));
        self.dirty(&crate::app::SKind::Modal);
        true
    }
}
