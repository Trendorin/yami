// Бар: тёмная матовая панель у любого края экрана (bar_pos: верх, низ, слева, справа), тонкая линия
// со стороны стола. Модули и их порядок — в настройках (bar_left / bar_center / bar_right); у бокового
// бара это верх, середина и низ столбца, а каждый модуль рисуется в узком виде (vmodule).

use crate::app::{App, SKind, State, BAR_H};
use crate::clock::{local_now, RU_DAY};
use crate::config::Settings;
use crate::font::{self, PX};
use crate::gfx::{BoxStyle, R};
use crate::icons;
use crate::theme::CLEAR;
use crate::ui::{Act, Cur, Tone, Ui};

const H: i32 = BAR_H;

/// тень плашки бара (плавает или уже экрана), px
pub const SHADE: i32 = 3;

/// где бар на мониторе длиной ow вдоль своего края (ширина у верхнего и нижнего, высота у бокового):
/// (отступ от своего края, отступ по бокам, толщина поверхности).
/// Якорь — свой край (bar_pos) и оба соседних, длина — отступами по бокам: так бар остаётся первым у niri и не
/// сужается чужими исключёнными зонами. Полоса во всю ширину — вплотную к краю, как раньше;
/// плашка (плавает или уже экрана) — с тенью, поверхность выше на её размер
pub fn geo(s: &Settings, ow: i32) -> (i32, i32, i32) {
    let m = if s.bar_float { s.num("bar_margin") } else { 0 };
    let pc = s.num("bar_width_pc");
    if strip(s) {
        return (0, 0, H);
    }
    let side = if pc >= 100 {
        m
    } else {
        // ширина плашки — доля монитора, не уже 240 px и не шире места между отступами
        // (на узком мониторе с большим отступом нижняя граница уступает верхней)
        let hi = (ow - 2 * m).max(1);
        let w = (ow * pc / 100).clamp(240.min(hi), hi);
        ((ow - w - SHADE) / 2).max(m)
    };
    (m, side, H + SHADE)
}

#[cfg(test)]
mod tests {
    #[test]
    fn shapes() {
        let mut s = crate::config::Settings::default();
        assert_eq!(super::geo(&s, 1920), (0, 0, super::H));
        s.bar_float = true;
        assert_eq!(super::geo(&s, 1920), (6, 6, super::H + super::SHADE));
        s.bar_width_pc = 60;
        let (top, side, _) = super::geo(&s, 1920);
        // 60 % монитора; поля по бокам одинаковые, поэтому ±1 px
        assert_eq!(top, 6);
        assert!((1920 - 2 * side - super::SHADE - 1152).abs() <= 1);
        // узкий монитор и большой отступ — без паники, плашка в пределах экрана
        s.bar_margin = 32;
        s.bar_width_pc = 30;
        for ow in [0, 1, 100, 250, 300] {
            assert!(super::geo(&s, ow).1 >= 0);
        }
        for ow in [640, 1280, 3840] {
            let (_, side, _) = super::geo(&s, ow);
            assert!(side >= 32 && 2 * side + 240 <= ow, "{ow}");
        }
    }

    #[test]
    fn rate_fits_three_signs() {
        use super::rate3;
        assert_eq!(rate3(0.0), "0K");
        assert_eq!(rate3(9.0 * 1024.0), "9K");
        assert_eq!(rate3(596.0 * 1024.0), ".6M");
        assert_eq!(rate3(12.0 * 1048576.0), "12M");
        assert_eq!(rate3(1.5 * 1073741824.0), "2G");
        for b in [0.0, 1e3, 1e5, 101_900.0, 1e6, 1e7, 1.04e8, 1e8, 1e9, 1e10, 1e11] {
            assert!(rate3(b).chars().count() <= 3, "{b} → {}", rate3(b));
        }
    }
}

/// бар у нижнего края экрана
pub fn low(s: &Settings) -> bool {
    s.bar_pos == "bottom"
}

/// бар у левого или правого края: столбец модулей
pub fn vert(s: &Settings) -> bool {
    matches!(s.bar_pos.as_str(), "left" | "right")
}

/// края экрана для бара по кругу в настройках: значение, подпись
pub const EDGES: &[(&str, &str)] = &[("top", "сверху"), ("bottom", "снизу"), ("left", "слева"), ("right", "справа")];

/// бар — полоса во всю ширину вплотную к краю (не плашка)
pub fn strip(s: &Settings) -> bool {
    !s.bar_float && s.num("bar_width_pc") >= 100
}

pub fn draw(ui: &mut Ui, st: &State, out: &str, w: i32, h: i32) {
    if vert(&st.set) {
        return draw_v(ui, st, out, h);
    }
    let t = ui.t;
    let set = &st.set;
    // пустое место бара (модули — поверх): ПКМ — панель управления
    ui.zone(R::new(0, 0, w, H), Act::BarBg, Cur::Default);
    let w = if strip(set) {
        ui.cv.blur_rect(R::new(0, 0, w, H), 0);
        ui.cv.fill(R::new(0, 0, w, H), t.glass_panel());
        // линия — со стороны стола: снизу у верхнего бара, сверху у нижнего
        ui.cv.fill(R::new(0, if low(set) { 0 } else { H - 1 }, w, 1), t.border);
        w
    } else {
        // плашка: рамка со всех сторон, фаска на углах, жёсткая тень (место под неё — справа и снизу)
        let pw = w - SHADE;
        ui.cv.pxbox(R::new(0, 0, pw, H), &BoxStyle::new(t.glass_panel(), t.border).shadow(SHADE));
        pw
    };

    let mid = set.num("bar_mid");
    let (pad_l, pad_r) = (set.num("bar_pad_l"), set.num("bar_pad_r"));
    // окна стола (#28) — модуль как все, в любой группе: своя ширина по числу окон, а когда места
    // не хватает, сжимается он один (кнопки уже, заголовки короче). Достаётся первой группе с ним
    let has = |l: &[String]| l.iter().any(|m| m == "tasks");
    let at = if has(&set.bar_left) { 0 } else if has(&set.bar_center) { 1 } else if has(&set.bar_right) { 2 } else { 3 };
    let nat = tasks_width(st, out, i32::MAX);
    let tw = |g: usize, full: i32| if g == at { full } else { 0 };
    let mut grow = |full: i32| {
        let lw = row(ui, st, out, &mut set.bar_left.iter(), 0, 1, false, tw(0, full));
        let cw = row(ui, st, out, &mut set.bar_center.iter(), 0, 1, false, tw(1, full));
        let rw = row(ui, st, out, &mut set.bar_right.iter(), 0, 1, false, tw(2, full));
        let mids = mid * ((lw > 0 && (cw > 0 || rw > 0)) as i32 + (cw > 0 && rw > 0) as i32);
        lw + cw + rw + mids
    };
    let over = if at < 3 && nat > 0 { grow(nat) - (w - pad_l - pad_r) } else { 0 };
    let tw_now = if over > 0 { tasks_width(st, out, nat - over) } else { nat };
    let left_end = row(ui, st, out, &mut set.bar_left.iter(), pad_l, 1, true, tw(0, tw_now));
    let right_start = row(ui, st, out, &mut set.bar_right.iter().rev(), w - pad_r, -1, true, tw(2, tw_now));

    // середина: между левыми и правыми модулями
    let x0 = if left_end > pad_l { left_end + mid } else { left_end };
    let x1 = if right_start < w - pad_r { right_start - mid } else { right_start };
    let ctw = tw(1, tw_now);
    let gw = row(ui, st, out, &mut set.bar_center.iter(), 0, 1, false, ctw);
    // по центру бара, как часы в большинстве шеллов; длинная левая группа (трек в «музыке»)
    // сдвигает середину ровно настолько, чтобы не наехать; after-left — сразу за левыми
    let cx = if set.bar_center_align == "after-left" { x0 } else { (w / 2 - gw / 2).clamp(x0, (x1 - gw).max(x0)) };
    ui.cv.push_clip(R::new(x0, 0, (x1 - x0).max(0), H));
    row(ui, st, out, &mut set.bar_center.iter(), cx, 1, true, ctw);
    ui.cv.pop_clip();
}

/// ряд модулей от x0 в сторону dir (1 — вправо, -1 — влево). Разделитель — только между двумя
/// видимыми модулями: без пар и висячих по краям. Возвращает внешний край последнего модуля (x0,
/// если модулей нет); draw = false — только меряет; tw — ширина окон стола в этом ряду (0 — их нет)
fn row(ui: &mut Ui, st: &State, out: &str, list: &mut dyn Iterator<Item = &String>, x0: i32, dir: i32, draw: bool, tw: i32) -> i32 {
    let (gap, sw) = (st.set.num("bar_gap"), st.set.num("bar_sep_w"));
    let mut x = x0;
    let (mut pending, mut any) = (false, false);
    for m in list {
        if m == "sep" {
            pending = any;
            continue;
        }
        let mw = if m == "tasks" { tw } else { module(ui, st, out, m, 0, true) };
        if mw == 0 {
            continue;
        }
        if any {
            x += dir * gap;
        }
        if pending {
            if draw {
                sep(ui, st, if dir > 0 { x } else { x - sw }, sw);
            }
            x += dir * (sw + gap);
            pending = false;
        }
        if draw {
            let mx = if dir > 0 { x } else { x - mw };
            if m == "tasks" {
                tasks(ui, st, out, R::new(mx, 0, mw, H - 1));
            } else {
                module(ui, st, out, m, mx, false);
            }
        }
        x += dir * mw;
        any = true;
    }
    x
}

/// разделитель шириной w от x: точки, черта, клетки или просто место
fn sep(ui: &mut Ui, st: &State, x: i32, w: i32) {
    let t = ui.t;
    let c = match st.set.bar_sep_color.as_str() {
        "accent" => t.red,
        "dim" => t.text_faint,
        _ => t.border,
    };
    let cx = x + w / 2 - 1;
    match st.set.bar_sep.as_str() {
        "line" => ui.cv.fill(R::new(cx, 7, 1, H - 14), c),
        "blocks" => {
            for i in 0..3 {
                ui.cv.fill(R::new(cx, H / 2 - 7 + i * 5, 3, 3), c);
            }
        }
        "gap" => {}
        _ => ui.cv.sep(cx, H / 2, c),
    }
}

/// значок программы трея 16×16 с левым верхним углом в x, y (бар и всплывашка `tray`)
/// значок программы трея; `pixel` (trayPixel, #19) — клетками в тонах палитры, как в доке и
/// лаунчере со «значками пиксельными»
pub fn tray_icon(ui: &mut Ui, it: &crate::dbus::TrayItem, x: i32, y: i32, pixel: bool) {
    let t = ui.t;
    // свой файл программы, потом имя в теме значков, потом её картинка (IconPixmap)
    let name = &it.icon;
    // пиксельному — исходник крупнее: сетка усредняется по площади
    let src = if pixel { 64 } else { 16 };
    let img = it.file.as_ref().and_then(|f| ui.ic.file(std::path::Path::new(f), src)).or_else(|| if name.is_empty() { None } else { ui.ic.get(name, src) }).or_else(|| it.pixmap.clone());
    match img {
        Some(img) if pixel => {
            let p = ui.ic.pixeled(&img, 16);
            ui.cv.image(&p, x, y, 1.0);
        }
        Some(img) => ui.cv.image(&img, x, y, 1.0),
        None => ui.cv.icon("dots", x + 1, y + 1, 2, t.text_dim),
    }
}

/// модуль бара: рисует (или только меряет) с левого края x, возвращает ширину
fn module(ui: &mut Ui, st: &State, out: &str, m: &str, x: i32, measure: bool) -> i32 {
    let t = ui.t;
    let cy = (H - 1) / 2;
    match m {
        "launcher" => {
            if !measure {
                let r = R::new(x, 0, 26, H - 1);
                let hov = ui.hover(r) || st.modal == Some(crate::app::Modal::Launcher);
                for i in 0..4 {
                    let c = if i == 0 || hov { t.red } else { t.dim };
                    ui.cv.fill(R::new(x + 6 + (i % 2) * 8, cy - 7 + (i / 2) * 8, 6, 6), c);
                }
                ui.zone(r, Act::Launcher, Cur::Pointer);
            }
            26
        }
        "workspaces" => {
            let list = st.niri.bar_ws(out);
            let w = list.len() as i32 * 26 - 4;
            if measure {
                return w.max(0);
            }
            let mut wx = x;
            for ws in list {
                let r = R::new(wx, cy - 11, 22, 22);
                let busy = st.niri.has_windows(ws.id);
                let hov = ui.hover(r);
                let fill = if ws.active { t.red } else if hov { t.surface_hi } else if busy { t.surface } else { CLEAR };
                let outline = if ws.urgent || ws.active { t.ember } else if busy { t.border } else { t.border.fade(0.5) };
                // срочный стол мигает
                let blink = ws.urgent && !ws.active && (st.started.elapsed().as_millis() / 400) % 2 == 1;
                if !blink {
                    ui.cv.pxbox(r, &BoxStyle::new(fill, outline).bevel(ws.active || busy).shadow(if ws.active { 2 } else { 0 }));
                    let label = ws.name.as_ref().and_then(|n| n.chars().next()).map(|c| c.to_string()).unwrap_or_else(|| ws.idx.to_string());
                    let c = if ws.active { t.on_accent } else if busy { t.text } else { t.text_faint };
                    ui.text_center(PX, r, &label, c);
                }
                if ws.urgent && !ws.active {
                    ui.anim = true;
                }
                ui.zone(r, Act::Ws(ws.id), Cur::Pointer);
                wx += 26;
            }
            w
        }
        "mode" => {
            let label = if st.niri.stacking { "стол" } else { "лента" };
            let w = ui.w(PX, label) + 14 + 7 + 14;
            if !measure {
                let r = R::new(x, cy - 11, w, 22);
                ui.button(r, label, if st.niri.stacking { "float" } else { "tile" }, Tone::Plain, st.niri.stacking, true, Act::Mode);
            }
            w
        }
        // трей стрелкой: одна кнопка, значки — во всплывашке (popups.rs `tray`); точка — кто-то зовёт
        "tray" if st.set.tray_mode == "arrow" => {
            if crate::popups::tray_items(st).next().is_none() {
                return 0;
            }
            if !measure {
                let r = R::new(x, cy - 11, 22, 22);
                let open = st.popup_is("tray");
                let hov = ui.hover(r);
                if hov || open {
                    ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, if open { t.red } else { t.border }));
                }
                ui.cv.icon(if open { "up" } else { "down" }, x + 4, cy - 7, 2, if hov || open { t.text_hi } else { t.text_dim });
                if crate::popups::tray_items(st).any(|i| i.status == "NeedsAttention") {
                    ui.cv.fill(R::new(x + 17, cy - 9, 4, 4), t.ember);
                }
                ui.zone(r, Act::Popup("tray"), Cur::Pointer);
            }
            22
        }
        "tray" => {
            let w = crate::popups::tray_items(st).count() as i32 * 24 - 2;
            if measure || w <= 0 {
                return w.max(0);
            }
            let mut ix = x;
            for it in crate::popups::tray_items(st) {
                let r = R::new(ix, cy - 11, 22, 22);
                if ui.hover(r) {
                    ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, t.red));
                }
                tray_icon(ui, it, ix + 3, cy - 8, st.set.tray_pixel);
                ui.zone(r, Act::Tray(it.service.clone()), Cur::Pointer);
                ix += 24;
            }
            w
        }
        "net" => {
            // значок wi-fi или кабеля: точка рядом с треем читалась как ещё один его значок (#8)
            let off = !st.sys.online;
            // по маршруту по умолчанию (sys.rs), а не по списку nmcli: тот есть, только пока открыта всплывашка
            let wifi = st.sys.wifi;
            let w = if off { 14 + 6 + ui.w(PX, "нет сети") } else { 14 };
            if !measure {
                let r = R::new(x - 2, 0, w + 4, H - 1);
                let hov = ui.hover(r);
                let c = if off { t.ember } else if hov { t.text_hi } else { t.text };
                ui.cv.icon(if wifi { "wifi" } else { "lan" }, x, cy - 7, 2, c);
                if off {
                    ui.text_in(PX, R::new(x + 20, 0, w, H - 1), "нет сети", t.ember);
                }
                ui.zone(r, Act::Popup("net"), Cur::Pointer);
            }
            w
        }
        "netspeed" => {
            let s = format!("↓{} ↑{}", crate::sys::rate(st.sys.rx), crate::sys::rate(st.sys.tx));
            let w = ui.w(PX, "↓8888K ↑8888K");
            if !measure {
                let r = R::new(x, 0, w, H - 1);
                ui.text_in(PX, r, &s, if ui.hover(r) { t.text } else { t.text_dim });
                ui.zone(r, Act::Popup("net"), Cur::Pointer);
            }
            w
        }
        "privacy" => crate::privacy::bar(ui, st, x, H, measure),
        "bt" => {
            if !st.bt.present {
                return 0;
            }
            let n = st.bt.devices.iter().filter(|d| d.connected).count();
            let label = if n > 0 { n.to_string() } else { String::new() };
            let w = 14 + if n > 0 { 4 + ui.w(PX, &label) } else { 0 };
            if !measure {
                let r = R::new(x - 2, 0, w + 4, H - 1);
                let hov = ui.hover(r);
                let c = if !st.bt.powered { t.text_faint } else if hov { t.text_hi } else if n > 0 { t.red } else { t.text_dim };
                ui.cv.icon("bt", x, cy - 7, 2, c);
                if n > 0 {
                    ui.text_in(PX, R::new(x + 18, 0, 40, H - 1), &label, t.text);
                }
                ui.zone(r, Act::Popup("bt"), Cur::Pointer);
            }
            w
        }
        "profile" => {
            if !measure {
                let r = R::new(x - 2, 0, 18, H - 1);
                let hov = ui.hover(r);
                let c = if hov { t.text_hi } else if !st.prof.present { t.text_faint } else if st.prof.active == "performance" { t.red } else if st.prof.active == "power-saver" { t.ok } else { t.text_dim };
                ui.cv.icon(st.prof.icon(), x, cy - 7, 2, c);
                ui.zone(r, Act::Popup("profile"), Cur::Pointer);
            }
            14
        }
        "bat" => {
            // батарея ноутбука: нет батареи — модуля нет. Значок — клетками по 2 px, как все значки бара
            let Some(bat) = st.sys.bat else { return 0 };
            // от сети и не заряжается (полная или держит порог): по желанию модуля нет (#21)
            if st.set.bat_hide_full && bat.plugged && !bat.charging {
                return 0;
            }
            let txt = format!("{}%", bat.pct);
            let bw = if bat.charging { 16 } else { 0 };
            let w = bw + 22 + 6 + ui.w(PX, &txt);
            if measure {
                return w;
            }
            let r = R::new(x - 2, 0, w + 4, H - 1);
            let hov = ui.hover(r);
            let low = bat.pct <= 15 && !bat.charging && !bat.plugged;
            let col = if low { t.ember } else if bat.charging || bat.plugged { t.ok } else { t.red };
            if bat.charging {
                ui.cv.icon("bolt", x, cy - 7, 2, if hov { t.text_hi } else { t.ok });
            }
            let body = if low { t.ember } else if hov { t.text_hi } else { t.text_dim };
            ui.cv.battery(x + bw, cy - 7, 2, bat.pct as f32 / 100.0, body, col, t.surface_hi.lerp(t.border, 0.45));
            ui.text_in(PX, R::new(x + bw + 28, 0, w, H - 1), &txt, if low { t.ember } else if hov { t.text } else { t.text_dim });
            ui.zone(r, Act::Popup("profile"), Cur::Pointer);
            w
        }
        "kb" => {
            let s = st.niri.kb_short();
            let s = if s.is_empty() { "??".to_string() } else { s };
            let w = ui.w(PX, &s) + 12;
            if !measure {
                ui.button(R::new(x, cy - 11, w, 22), &s, "", Tone::Plain, false, true, Act::Kb);
            }
            w
        }
        // температура коротко: столбик-градусник и число, без подписи и шкалы
        "temp" if st.set.temp_style == "compact" => {
            let txt = format!("{}°", st.sys.temp.round());
            let hot = st.sys.temp > 80.0;
            let tw = ui.w(PX, "88°").max(ui.w(PX, &txt));
            let w = 4 + 5 + tw;
            if measure {
                return w;
            }
            let r = R::new(x, 0, w, H - 1);
            let hov = ui.hover(r);
            // 7 клеток по 2 px: целые пиксели, без полутонов
            let cells = ((st.sys.temp / 100.0).clamp(0.0, 1.0) * 7.0).round() as i32;
            ui.cv.fill(R::new(x, cy - 7, 4, 14), t.surface_hi.lerp(t.border, 0.45));
            ui.cv.fill(R::new(x, cy + 7 - cells * 2, 4, cells * 2), if hot { t.ember } else { t.red });
            ui.text_in(PX, R::new(x + 9, 0, tw, H - 1), &txt, if hot { t.ember } else if hov { t.text } else { t.text_dim });
            ui.zone(r, Act::Popup("sys"), Cur::Pointer);
            w
        }
        "cpu" | "ram" | "temp" | "gpu" | "disk" => {
            // видеокарту нечем спросить (нет amdgpu и nvidia-smi) — модуля нет
            if m == "gpu" && !st.sys.has_gpu {
                return 0;
            }
            let s = &st.sys;
            let (label, v, txt, hot) = match m {
                "cpu" => ("цп", s.cpu, format!("{}%", (s.cpu * 100.0).round()), s.cpu > 0.85),
                "ram" => ("озу", s.mem, format!("{}%", (s.mem * 100.0).round()), s.mem > 0.85),
                // спящую дискретку ноутбука не будим (sys.rs): «сон» вместо цифр
                "gpu" if s.gpu_sleep => ("гп", 0.0, crate::i18n::tr("сон").to_string(), false),
                "gpu" => ("гп", s.gpu_load, format!("{}%", (s.gpu_load * 100.0).round()), s.gpu_load > 0.95 || s.gpu_temp > 85.0),
                "disk" => ("диск", s.disk, format!("{}%", (s.disk * 100.0).round()), s.disk > 0.9),
                _ => ("t°", (s.temp / 100.0).min(1.0), format!("{}°", s.temp.round()), s.temp > 80.0),
            };
            let lw = ui.w(PX, label);
            // у диска истории нет: «график» — шкалой
            let style = if m == "disk" && st.set.cpu_style == "graph" { "gauge" } else { st.set.cpu_style.as_str() };
            let gw = if style == "number" { 0 } else { 48 };
            let w = lw + 6 + gw + if gw > 0 { 6 } else { 0 } + 34;
            if measure {
                return w;
            }
            let r = R::new(x, 0, w, H - 1);
            let hov = ui.hover(r);
            ui.text_in(PX, R::new(x, 0, lw, H - 1), label, if hov { t.text_dim } else { t.text_faint });
            let gx = x + lw + 6;
            let col = if hot { t.ember } else { t.red };
            if style == "graph" {
                let src = match m {
                    "cpu" => "cpu",
                    "ram" => "ram",
                    "gpu" => "gpu_load",
                    _ => "temp",
                };
                let (ser, full) = st.sys.series(src);
                let pts = ser.tail(24);
                for (i, p) in pts.iter().enumerate() {
                    let hh = ((p / full).clamp(0.0, 1.0) * 14.0).round() as i32;
                    ui.cv.fill(R::new(gx + i as i32 * 2, cy + 7 - hh.max(1), 2, hh.max(1)), if i == pts.len() - 1 { t.ember } else { col.fade(0.8) });
                }
                ui.cv.fill(R::new(gx, cy + 8, 48, 1), t.border);
            } else if style == "gauge" {
                ui.cv.gauge(R::new(gx, cy - 5, 48, 10), 6, v, col, t.surface_hi.lerp(t.border, 0.45), 2);
            }
            let tx = gx + gw + if gw > 0 { 6 } else { 0 };
            ui.text_in(PX, R::new(tx, 0, 34, H - 1), &txt, if hot { t.ember } else { t.text_dim });
            ui.zone(r, Act::Popup("sys"), Cur::Pointer);
            w
        }
        "vol" => {
            let a = &st.audio;
            let s = if a.muted { "—".to_string() } else { format!("{}", (a.volume * 100.0).round()) };
            let w = 14 + 5 + ui.w(PX, "100");
            if !measure {
                let r = R::new(x, 0, w, H - 1);
                let hov = ui.hover(r);
                ui.cv.icon(if a.muted { "mute" } else { "speaker" }, x, cy - 7, 2, if a.muted { t.text_faint } else if hov { t.text_hi } else { t.text });
                ui.text_in(PX, R::new(x + 19, 0, w - 19, H - 1), &s, if a.muted { t.text_faint } else { t.text });
                ui.zone(r, Act::Popup("sound"), Cur::Pointer);
            }
            w
        }
        "media" => {
            let me = &st.media;
            if me.title.is_empty() {
                return 0;
            }
            let w = 200;
            if !measure {
                let r = R::new(x, 0, w, H - 1);
                let hov = ui.hover(r);
                // маленький эквалайзер: 5 столбиков (mediaViz = off — без него, #13)
                let eq = st.set.media_viz != "off";
                if eq {
                    for i in 0..5 {
                        let v = st.viz.get(i * 4 + 2).copied().unwrap_or(0.0);
                        let hh = if me.playing() { (2.0 + v * 12.0).round() as i32 } else { 2 };
                        ui.cv.fill(R::new(x + i as i32 * 3, cy + 7 - hh, 2, hh), t.red);
                    }
                } else {
                    ui.cv.icon("music", x, cy - 6, 2, if me.playing() { t.red } else { t.text_faint });
                }
                let label = if me.artist.is_empty() { me.title.clone() } else { format!("{} — {}", me.artist, me.title) };
                ui.text_in(PX, R::new(x + 20, 0, w - 20, H - 1), &label, if hov { t.text_hi } else { t.text_dim });
                ui.zone(r, Act::Popup("media"), Cur::Pointer);
            }
            w
        }
        "clock" => {
            let tm = local_now();
            let time = if st.set.clock_seconds { format!("{:02}:{:02}:{:02}", tm.hour, tm.min, tm.sec) } else { format!("{:02}:{:02}", tm.hour, tm.min) };
            let date = format!("{} {:02}.{:02}", crate::i18n::tr(RU_DAY[tm.wday as usize]), tm.day, tm.mon);
            let tw = ui.w(PX, &time);
            let dw = if st.set.clock_date { ui.w(PX, &date) + 6 } else { 0 };
            let w = tw + dw;
            if !measure {
                let r = R::new(x - 4, 0, w + 8, H - 1);
                let hov = ui.hover(r) || st.popup_is("cal");
                if hov {
                    ui.cv.fill(R::new(x - 4, H - 3, w + 8, 2), t.red);
                }
                ui.text_in(PX, R::new(x, 0, tw, H - 1), &time, if hov { t.text_hi } else { t.text });
                if st.set.clock_date {
                    ui.text_in(PX, R::new(x + tw + 6, 0, dw, H - 1), &date, t.text_dim);
                }
                ui.zone(r, Act::Popup("cal"), Cur::Pointer);
            }
            w
        }
        "night" => {
            if !measure {
                let r = R::new(x - 2, 0, 18, H - 1);
                let hov = ui.hover(r);
                let c = if hov { t.text_hi } else if st.night.on { t.warn } else { t.text_faint };
                ui.cv.icon("moon", x, cy - 7, 2, c);
                ui.zone(r, Act::NightToggle, Cur::Pointer);
            }
            14
        }
        "weather" => {
            // погода (weather.rs): значок и температура; города нет или ответа ещё нет — «?»
            let wt = crate::weather::ready(st);
            let txt = wt.map(|w| crate::weather::deg(w.temp)).unwrap_or_else(|| "?".into());
            let w = 16 + ui.w(PX, &txt);
            if !measure {
                let r = R::new(x - 2, 0, w + 4, H - 1);
                let hov = ui.hover(r);
                let c = if hov { t.text_hi } else if wt.is_some() { t.text } else { t.text_faint };
                ui.cv.icon(wt.map(|w| crate::weather::icon(w.code, w.day)).unwrap_or("cloud"), x, cy - 7, 2, c);
                ui.cv.text_in(ui.f, PX, R::new(x + 16, 0, w - 14, H - 1), &txt, c);
                ui.zone(r, Act::Popup("weather"), Cur::Pointer);
            }
            w
        }
        m if m.starts_with("x:") => {
            // модуль-скрипт: значок 7×7 и текст его цветом роли
            let Some(p) = st.plugins.shown.get(m) else { return 0 };
            if p.text.is_empty() && p.icon.is_empty() {
                return 0;
            }
            let icon = !p.icon.is_empty() && crate::gfx::icon7(&p.icon).is_some();
            // в баре — первая строка, не длиннее 48 знаков (многострочное — для виджета)
            let text: String = p.text.lines().next().unwrap_or("").chars().take(48).collect();
            let tw = ui.f.width(PX, &text);
            let w = if icon { 16 } else { 0 } + tw;
            if !measure {
                let r = R::new(x - 2, 0, w + 4, H - 1);
                let hov = !p.click.is_empty() && ui.hover(r);
                let c = if hov { t.text_hi } else { crate::plugins::color(&t, &p.color) };
                if icon {
                    ui.cv.icon(&p.icon, x, cy - 7, 2, c);
                }
                ui.cv.text_in(ui.f, PX, R::new(x + if icon { 16 } else { 0 }, 0, tw + 2, H - 1), &text, c);
                if !p.click.is_empty() {
                    ui.zone(r, Act::Plug(p.click.clone()), Cur::Pointer);
                }
            }
            w
        }
        "notif" => {
            let n = st.notifs.history.len();
            let w = 14 + if n > 0 { 4 + ui.w(PX, &n.to_string()) } else { 0 };
            if !measure {
                let r = R::new(x - 2, 0, w + 4, H - 1);
                let hov = ui.hover(r);
                let c = if st.set.dnd { t.text_faint } else if hov { t.text_hi } else if n > 0 { t.red } else { t.text_dim };
                ui.cv.icon(if st.set.dnd { "bellx" } else { "bell" }, x, cy - 7, 2, c);
                if n > 0 {
                    ui.text_in(PX, R::new(x + 18, 0, 30, H - 1), &n.to_string(), t.text);
                }
                ui.zone(r, Act::Popup("cal"), Cur::Pointer);
            }
            w
        }
        "settings" => {
            if !measure {
                ui.button(R::new(x, cy - 11, 26, 22), "", "gear", Tone::Ghost, st.modal == Some(crate::app::Modal::Settings), true, Act::Settings);
            }
            26
        }
        "power" => {
            if !measure {
                ui.button(R::new(x, cy - 11, 26, 22), "", "power", Tone::Danger, st.modal == Some(crate::app::Modal::Power), true, Act::Power);
            }
            26
        }
        _ => 0,
    }
}

// ── боковой бар ──────────────────────────────────────────────────────────────
// Столбец толщиной H: левая группа сверху, правая снизу, середина по центру. Ширина модулей одна
// (W), высота своя; числа без подписей единиц, где они не влезают, часы — часами над минутами.

/// ширина содержимого столбца (линия со стороны стола — вне её)
const W: i32 = H - 1;

/// x содержимого: у левого бара линия справа, у правого слева
fn vx(st: &State) -> i32 {
    (st.set.bar_pos == "right") as i32
}

/// боковой бар высотой h: фон, три группы модулей сверху вниз
fn draw_v(ui: &mut Ui, st: &State, out: &str, h: i32) {
    let t = ui.t;
    let set = &st.set;
    ui.zone(R::new(0, 0, H, h), Act::BarBg, Cur::Default);
    let h = if strip(set) {
        ui.cv.blur_rect(R::new(0, 0, H, h), 0);
        ui.cv.fill(R::new(0, 0, H, h), t.glass_panel());
        ui.cv.fill(R::new(if set.bar_pos == "right" { 0 } else { H - 1 }, 0, 1, h), t.border);
        h
    } else {
        let ph = h - SHADE;
        ui.cv.pxbox(R::new(0, 0, H, ph), &BoxStyle::new(t.glass_panel(), t.border).shadow(SHADE));
        ph
    };
    let mid = set.num("bar_mid");
    let (pad_t, pad_b) = (set.num("bar_pad_l"), set.num("bar_pad_r"));
    // окна стола сжимаются, как у верхнего бара: сначала все группы без них, остаток — им
    let has = |l: &[String]| l.iter().any(|m| m == "tasks");
    let at = if has(&set.bar_left) { 0 } else if has(&set.bar_center) { 1 } else if has(&set.bar_right) { 2 } else { 3 };
    let th = |g: usize, full: i32| if g == at { full } else { 0 };
    let nat = vtasks_h(st, out, i32::MAX);
    let mut need = |full: i32| {
        let a = col(ui, st, out, &mut set.bar_left.iter(), 0, 1, false, th(0, full));
        let b = col(ui, st, out, &mut set.bar_center.iter(), 0, 1, false, th(1, full));
        let c = col(ui, st, out, &mut set.bar_right.iter(), 0, 1, false, th(2, full));
        a + b + c + mid * ((a > 0 && (b > 0 || c > 0)) as i32 + (b > 0 && c > 0) as i32)
    };
    let over = if at < 3 && nat > 0 { need(nat) - (h - pad_t - pad_b) } else { 0 };
    let th_now = if over > 0 { vtasks_h(st, out, nat - over) } else { nat };
    let top_end = col(ui, st, out, &mut set.bar_left.iter(), pad_t, 1, true, th(0, th_now));
    let bot_start = col(ui, st, out, &mut set.bar_right.iter().rev(), h - pad_b, -1, true, th(2, th_now));
    let y0 = if top_end > pad_t { top_end + mid } else { top_end };
    let y1 = if bot_start < h - pad_b { bot_start - mid } else { bot_start };
    let cth = th(1, th_now);
    let gh = col(ui, st, out, &mut set.bar_center.iter(), 0, 1, false, cth);
    let cy = if set.bar_center_align == "after-left" { y0 } else { (h / 2 - gh / 2).clamp(y0, (y1 - gh).max(y0)) };
    ui.cv.push_clip(R::new(0, y0, H, (y1 - y0).max(0)));
    col(ui, st, out, &mut set.bar_center.iter(), cy, 1, true, cth);
    ui.cv.pop_clip();
}

/// столбец модулей от y0 вниз (dir 1) или вверх (-1); как `row`, только по вертикали
fn col(ui: &mut Ui, st: &State, out: &str, list: &mut dyn Iterator<Item = &String>, y0: i32, dir: i32, draw: bool, th: i32) -> i32 {
    let (gap, sw) = (st.set.num("bar_gap"), st.set.num("bar_sep_w"));
    let mut y = y0;
    let (mut pending, mut any) = (false, false);
    for m in list {
        if m == "sep" {
            pending = any;
            continue;
        }
        let mh = if m == "tasks" { th } else { vmodule(ui, st, out, m, 0, true) };
        if mh == 0 {
            continue;
        }
        if any {
            y += dir * gap;
        }
        if pending {
            if draw {
                vsep(ui, st, if dir > 0 { y } else { y - sw }, sw);
            }
            y += dir * (sw + gap);
            pending = false;
        }
        if draw {
            let my = if dir > 0 { y } else { y - mh };
            if m == "tasks" {
                vtasks(ui, st, out, R::new(vx(st), my, W, mh));
            } else {
                vmodule(ui, st, out, m, my, false);
            }
        }
        y += dir * mh;
        any = true;
    }
    y
}

/// разделитель бокового бара: те же точки, черта и клетки, только поперёк
fn vsep(ui: &mut Ui, st: &State, y: i32, h: i32) {
    let t = ui.t;
    let c = match st.set.bar_sep_color.as_str() {
        "accent" => t.red,
        "dim" => t.text_faint,
        _ => t.border,
    };
    let (x, cy) = (vx(st), y + h / 2 - 1);
    match st.set.bar_sep.as_str() {
        "line" => ui.cv.fill(R::new(x + 7, cy, W - 14, 1), c),
        "blocks" => {
            for i in 0..3 {
                ui.cv.fill(R::new(x + W / 2 - 7 + i * 5, cy, 3, 3), c);
            }
        }
        "gap" => {}
        _ => {
            for i in 0..4 {
                ui.cv.fill(R::new(x + W / 2 - 8 + i * 4, cy, 2, 2), c);
            }
        }
    }
}

/// значок 7×7 клетками по 2 px по центру столбца на y
fn vicon(ui: &mut Ui, st: &State, name: &str, y: i32, c: crate::gfx::C) {
    ui.cv.icon(name, vx(st) + W / 2 - 7, y, 2, c);
}

/// строка текста по центру столбца на y (14 px)
fn vtext(ui: &mut Ui, st: &State, y: i32, s: &str, c: crate::gfx::C) {
    ui.text_center(PX, R::new(vx(st), y, W, 14), s, c);
}

/// значок и под ним короткая подпись (громкость, погода, батарея…): высота 30 или 14 без подписи
fn vstack(ui: &mut Ui, st: &State, y: i32, icon: &str, ic: crate::gfx::C, label: &str, lc: crate::gfx::C, act: Act, measure: bool) -> i32 {
    let h = if label.is_empty() { 14 } else { 30 };
    if !measure {
        vicon(ui, st, icon, y, ic);
        if !label.is_empty() {
            vtext(ui, st, y + 16, label, lc);
        }
        ui.zone(R::new(vx(st), y - 2, W, h + 4), act, Cur::Pointer);
    }
    h
}

/// скорость не длиннее трёх знаков — столбец узкий: 9K, 596K → .6M, 12M, 1.5G → 2G
pub fn rate3(b: f64) -> String {
    let k = b / 1024.0;
    let m = k / 1024.0;
    let g = m / 1024.0;
    if k < 99.5 {
        format!("{}K", k.round() as i64)
    } else if m < 0.95 {
        format!(".{}M", ((m * 10.0).round() as i64).max(1))
    } else if m < 99.5 {
        format!("{}M", m.round() as i64)
    } else if g < 0.95 {
        format!(".{}G", ((g * 10.0).round() as i64).max(1))
    } else {
        format!("{}G", g.round() as i64)
    }
}

/// модуль бокового бара: рисует (или только меряет) с верхнего края y, возвращает высоту
fn vmodule(ui: &mut Ui, st: &State, out: &str, m: &str, y: i32, measure: bool) -> i32 {
    let t = ui.t;
    let (x0, cx) = (vx(st), vx(st) + W / 2);
    // полоса под мышью — со стороны стола, как подчёркивание часов у верхнего бара
    let mark = |ui: &mut Ui, r: R| {
        if ui.hover(r) {
            let lx = if st.set.bar_pos == "right" { x0 } else { x0 + W - 2 };
            ui.cv.fill(R::new(lx, r.y, 2, r.h), t.red);
        }
    };
    match m {
        "launcher" => {
            if !measure {
                let r = R::new(x0, y, W, 26);
                let hov = ui.hover(r) || st.modal == Some(crate::app::Modal::Launcher);
                for i in 0..4 {
                    let c = if i == 0 || hov { t.red } else { t.dim };
                    ui.cv.fill(R::new(cx - 7 + (i % 2) * 8, y + 6 + (i / 2) * 8, 6, 6), c);
                }
                ui.zone(r, Act::Launcher, Cur::Pointer);
            }
            26
        }
        "workspaces" => {
            let list = st.niri.bar_ws(out);
            let h = (list.len() as i32 * 26 - 4).max(0);
            if measure {
                return h;
            }
            for (i, ws) in list.into_iter().enumerate() {
                let r = R::new(cx - 11, y + i as i32 * 26, 22, 22);
                let busy = st.niri.has_windows(ws.id);
                let hov = ui.hover(r);
                let fill = if ws.active { t.red } else if hov { t.surface_hi } else if busy { t.surface } else { CLEAR };
                let outline = if ws.urgent || ws.active { t.ember } else if busy { t.border } else { t.border.fade(0.5) };
                let blink = ws.urgent && !ws.active && (st.started.elapsed().as_millis() / 400) % 2 == 1;
                if !blink {
                    ui.cv.pxbox(r, &BoxStyle::new(fill, outline).bevel(ws.active || busy).shadow(if ws.active { 2 } else { 0 }));
                    let label = ws.name.as_ref().and_then(|n| n.chars().next()).map(|c| c.to_string()).unwrap_or_else(|| ws.idx.to_string());
                    let c = if ws.active { t.on_accent } else if busy { t.text } else { t.text_faint };
                    ui.text_center(PX, r, &label, c);
                }
                if ws.urgent && !ws.active {
                    ui.anim = true;
                }
                ui.zone(r, Act::Ws(ws.id), Cur::Pointer);
            }
            h
        }
        "mode" => {
            if !measure {
                ui.button(R::new(cx - 11, y, 22, 22), "", if st.niri.stacking { "float" } else { "tile" }, Tone::Plain, st.niri.stacking, true, Act::Mode);
            }
            22
        }
        "tray" if st.set.tray_mode == "arrow" => {
            if crate::popups::tray_items(st).next().is_none() {
                return 0;
            }
            if !measure {
                let r = R::new(cx - 11, y, 22, 22);
                let open = st.popup_is("tray");
                let hov = ui.hover(r);
                if hov || open {
                    ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, if open { t.red } else { t.border }));
                }
                // стрелка — в сторону, где встанет всплывашка
                let away = if st.set.bar_pos == "right" { "left" } else { "right" };
                let back = if away == "left" { "right" } else { "left" };
                ui.cv.icon(if open { back } else { away }, cx - 7, y + 4, 2, if hov || open { t.text_hi } else { t.text_dim });
                if crate::popups::tray_items(st).any(|i| i.status == "NeedsAttention") {
                    ui.cv.fill(R::new(cx + 6, y + 2, 4, 4), t.ember);
                }
                ui.zone(r, Act::Popup("tray"), Cur::Pointer);
            }
            22
        }
        "tray" => {
            let h = crate::popups::tray_items(st).count() as i32 * 24 - 2;
            if measure || h <= 0 {
                return h.max(0);
            }
            for (i, it) in crate::popups::tray_items(st).enumerate() {
                let r = R::new(cx - 11, y + i as i32 * 24, 22, 22);
                if ui.hover(r) {
                    ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, t.red));
                }
                tray_icon(ui, it, r.x + 3, r.y + 3, st.set.tray_pixel);
                ui.zone(r, Act::Tray(it.service.clone()), Cur::Pointer);
            }
            h
        }
        "net" => {
            let off = !st.sys.online;
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 18));
            let c = if off { t.ember } else if hov { t.text_hi } else { t.text };
            vstack(ui, st, y, if st.sys.wifi { "wifi" } else { "lan" }, c, "", c, Act::Popup("net"), measure)
        }
        "netspeed" => {
            if !measure {
                let r = R::new(x0, y, W, 30);
                let hov = ui.hover(r);
                vtext(ui, st, y, &rate3(st.sys.rx), if hov { t.text_hi } else { t.text });
                vtext(ui, st, y + 16, &rate3(st.sys.tx), if hov { t.text } else { t.text_dim });
                ui.zone(r, Act::Popup("net"), Cur::Pointer);
            }
            30
        }
        "privacy" => crate::privacy::vbar(ui, st, x0, W, y, measure),
        "bt" => {
            if !st.bt.present {
                return 0;
            }
            let n = st.bt.devices.iter().filter(|d| d.connected).count();
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 34));
            let c = if !st.bt.powered { t.text_faint } else if hov { t.text_hi } else if n > 0 { t.red } else { t.text_dim };
            vstack(ui, st, y, "bt", c, &if n > 0 { n.to_string() } else { String::new() }, t.text, Act::Popup("bt"), measure)
        }
        "profile" => {
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 18));
            let c = if hov { t.text_hi } else if !st.prof.present { t.text_faint } else if st.prof.active == "performance" { t.red } else if st.prof.active == "power-saver" { t.ok } else { t.text_dim };
            vstack(ui, st, y, st.prof.icon(), c, "", c, Act::Popup("profile"), measure)
        }
        "bat" => {
            let Some(bat) = st.sys.bat else { return 0 };
            if st.set.bat_hide_full && bat.plugged && !bat.charging {
                return 0;
            }
            let bh = if bat.charging { 16 } else { 0 };
            let h = bh + 14 + 2 + 14;
            if measure {
                return h;
            }
            let r = R::new(x0, y - 2, W, h + 4);
            let hov = ui.hover(r);
            let low = bat.pct <= 15 && !bat.charging && !bat.plugged;
            let col = if low { t.ember } else if bat.charging || bat.plugged { t.ok } else { t.red };
            if bat.charging {
                vicon(ui, st, "bolt", y, if hov { t.text_hi } else { t.ok });
            }
            let body = if low { t.ember } else if hov { t.text_hi } else { t.text_dim };
            ui.cv.battery(cx - 11, y + bh, 2, bat.pct as f32 / 100.0, body, col, t.surface_hi.lerp(t.border, 0.45));
            vtext(ui, st, y + bh + 16, &bat.pct.to_string(), if low { t.ember } else if hov { t.text } else { t.text_dim });
            ui.zone(r, Act::Popup("profile"), Cur::Pointer);
            h
        }
        "kb" => {
            if !measure {
                let s = st.niri.kb_short();
                let s = if s.is_empty() { "??".to_string() } else { s };
                let bw = (ui.w(PX, &s) + 8).min(W - 2);
                ui.button(R::new(cx - bw / 2, y, bw, 22), &s, "", Tone::Plain, false, true, Act::Kb);
            }
            22
        }
        "cpu" | "ram" | "temp" | "gpu" | "disk" => {
            if m == "gpu" && !st.sys.has_gpu {
                return 0;
            }
            let s = &st.sys;
            // подпись — значком: слово на любом языке в столбец шириной 31 px не влезает
            let (icon, v, txt, hot) = match m {
                "cpu" => ("cpu", s.cpu, format!("{}", (s.cpu * 100.0).round()), s.cpu > 0.85),
                "ram" => ("ram", s.mem, format!("{}", (s.mem * 100.0).round()), s.mem > 0.85),
                "gpu" if s.gpu_sleep => ("screen", 0.0, "z".to_string(), false),
                "gpu" => ("screen", s.gpu_load, format!("{}", (s.gpu_load * 100.0).round()), s.gpu_load > 0.95 || s.gpu_temp > 85.0),
                "disk" => ("save", s.disk, format!("{}", (s.disk * 100.0).round()), s.disk > 0.9),
                _ => ("temp", (s.temp / 100.0).min(1.0), format!("{}°", s.temp.round()), s.temp > 80.0),
            };
            // коротко (temp_style) — градусник вместо подписи и шкалы
            let compact = m == "temp" && st.set.temp_style == "compact";
            let style = if compact { "number" } else if m == "disk" && st.set.cpu_style == "graph" { "gauge" } else { st.set.cpu_style.as_str() };
            let gh = if style == "number" { 0 } else { 12 };
            let h = if compact { 14 + 2 + 14 } else { 16 + gh + 14 };
            if measure {
                return h;
            }
            let r = R::new(x0, y, W, h);
            let hov = ui.hover(r);
            let col = if hot { t.ember } else { t.red };
            let mut ty = y;
            if compact {
                let cells = ((st.sys.temp / 100.0).clamp(0.0, 1.0) * 7.0).round() as i32;
                ui.cv.fill(R::new(cx - 2, y, 4, 14), t.surface_hi.lerp(t.border, 0.45));
                ui.cv.fill(R::new(cx - 2, y + 14 - cells * 2, 4, cells * 2), col);
                ty += 16;
            } else {
                vicon(ui, st, icon, y, if hov { t.text_dim } else { t.text_faint });
                ty += 16;
                let gx = cx - 12;
                if style == "graph" {
                    let src = match m {
                        "cpu" => "cpu",
                        "ram" => "ram",
                        "gpu" => "gpu_load",
                        _ => "temp",
                    };
                    let (ser, full) = st.sys.series(src);
                    let pts = ser.tail(12);
                    for (i, p) in pts.iter().enumerate() {
                        let hh = ((p / full).clamp(0.0, 1.0) * 8.0).round() as i32;
                        ui.cv.fill(R::new(gx + i as i32 * 2, ty + 9 - hh.max(1), 2, hh.max(1)), if i == pts.len() - 1 { t.ember } else { col.fade(0.8) });
                    }
                } else if style == "gauge" {
                    ui.cv.gauge(R::new(gx, ty + 2, 24, 6), 4, v, col, t.surface_hi.lerp(t.border, 0.45), 2);
                }
                ty += gh;
            }
            vtext(ui, st, ty, &txt, if hot { t.ember } else if hov { t.text } else { t.text_dim });
            ui.zone(r, Act::Popup("sys"), Cur::Pointer);
            h
        }
        "vol" => {
            let a = &st.audio;
            let s = if a.muted { "—".to_string() } else { format!("{}", (a.volume * 100.0).round()) };
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 34));
            let ic = if a.muted { t.text_faint } else if hov { t.text_hi } else { t.text };
            vstack(ui, st, y, if a.muted { "mute" } else { "speaker" }, ic, &s, if a.muted { t.text_faint } else { t.text }, Act::Popup("sound"), measure)
        }
        "media" => {
            let me = &st.media;
            if me.title.is_empty() {
                return 0;
            }
            if !measure {
                let r = R::new(x0, y - 2, W, 18);
                mark(ui, r);
                if st.set.media_viz != "off" {
                    for i in 0..5 {
                        let v = st.viz.get(i * 4 + 2).copied().unwrap_or(0.0);
                        let hh = if me.playing() { (2.0 + v * 12.0).round() as i32 } else { 2 };
                        ui.cv.fill(R::new(cx - 7 + i as i32 * 3, y + 14 - hh, 2, hh), t.red);
                    }
                } else {
                    vicon(ui, st, "music", y, if me.playing() { t.red } else { t.text_faint });
                }
                ui.zone(r, Act::Popup("media"), Cur::Pointer);
            }
            14
        }
        "clock" => {
            let tm = local_now();
            let mut parts = vec![format!("{:02}", tm.hour), format!("{:02}", tm.min)];
            if st.set.clock_seconds {
                parts.push(format!("{:02}", tm.sec));
            }
            let lines = parts.len() as i32;
            let date = st.set.clock_date;
            let h = lines * 14 + (lines - 1) * 2 + if date { 6 + 14 } else { 0 };
            if !measure {
                let r = R::new(x0, y - 2, W, h + 4);
                let hov = ui.hover(r) || st.popup_is("cal");
                if hov {
                    let lx = if st.set.bar_pos == "right" { x0 } else { x0 + W - 2 };
                    ui.cv.fill(R::new(lx, r.y, 2, r.h), t.red);
                }
                for (i, p) in parts.iter().enumerate() {
                    vtext(ui, st, y + i as i32 * 16, p, if hov { t.text_hi } else { t.text });
                }
                if date {
                    // число месяца под часами, отделённое чертой
                    let dy = y + lines * 16;
                    ui.cv.fill(R::new(cx - 5, dy + 1, 10, 1), t.border);
                    vtext(ui, st, dy + 6, &format!("{:02}", tm.day), t.text_dim);
                }
                ui.zone(r, Act::Popup("cal"), Cur::Pointer);
            }
            h
        }
        "night" => {
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 18));
            let c = if hov { t.text_hi } else if st.night.on { t.warn } else { t.text_faint };
            vstack(ui, st, y, "moon", c, "", c, Act::NightToggle, measure)
        }
        "weather" => {
            let wt = crate::weather::ready(st);
            let txt = wt.map(|w| crate::weather::deg(w.temp)).unwrap_or_else(|| "?".into());
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 34));
            let c = if hov { t.text_hi } else if wt.is_some() { t.text } else { t.text_faint };
            vstack(ui, st, y, wt.map(|w| crate::weather::icon(w.code, w.day)).unwrap_or("cloud"), c, &txt, c, Act::Popup("weather"), measure)
        }
        m if m.starts_with("x:") => {
            let Some(p) = st.plugins.shown.get(m) else { return 0 };
            if p.text.is_empty() && p.icon.is_empty() {
                return 0;
            }
            let icon = !p.icon.is_empty() && crate::gfx::icon7(&p.icon).is_some();
            // столбец узкий: столько знаков первой строки, сколько влезает
            let mut text = String::new();
            for ch in p.text.lines().next().unwrap_or("").chars().take(8) {
                text.push(ch);
                if ui.f.width(PX, &text) > W - 2 {
                    text.pop();
                    break;
                }
            }
            let h = if icon { 14 } else { 0 } + if text.is_empty() { 0 } else if icon { 16 } else { 14 };
            if h == 0 || measure {
                return h;
            }
            let r = R::new(x0, y - 2, W, h + 4);
            let hov = !p.click.is_empty() && ui.hover(r);
            let c = if hov { t.text_hi } else { crate::plugins::color(&t, &p.color) };
            if icon {
                vicon(ui, st, &p.icon, y, c);
            }
            if !text.is_empty() {
                ui.cv.text_center(ui.f, PX, R::new(x0, y + if icon { 16 } else { 0 }, W, 14), &text, c);
            }
            if !p.click.is_empty() {
                ui.zone(r, Act::Plug(p.click.clone()), Cur::Pointer);
            }
            h
        }
        "notif" => {
            let n = st.notifs.history.len();
            let hov = !measure && ui.hover(R::new(x0, y - 2, W, 34));
            let c = if st.set.dnd { t.text_faint } else if hov { t.text_hi } else if n > 0 { t.red } else { t.text_dim };
            vstack(ui, st, y, if st.set.dnd { "bellx" } else { "bell" }, c, &if n > 0 { n.min(99).to_string() } else { String::new() }, t.text, Act::Popup("cal"), measure)
        }
        "settings" => {
            if !measure {
                ui.button(R::new(cx - 12, y, 24, 22), "", "gear", Tone::Ghost, st.modal == Some(crate::app::Modal::Settings), true, Act::Settings);
            }
            22
        }
        "power" => {
            if !measure {
                ui.button(R::new(cx - 12, y, 24, 22), "", "power", Tone::Danger, st.modal == Some(crate::app::Modal::Power), true, Act::Power);
            }
            22
        }
        _ => 0,
    }
}

/// окна стола в боковом баре — квадратами со значком; высота не больше max (0 — нет окон или места)
const VTASK: i32 = 26;
fn vtasks_h(st: &State, out: &str, max: i32) -> i32 {
    let n = task_list(st, out).len() as i32;
    let tg = st.set.num("bar_task_gap");
    let fit = (max.saturating_add(tg) / (VTASK + tg)).clamp(0, n);
    if fit == 0 {
        return 0;
    }
    fit * VTASK + (fit - 1) * tg
}

fn vtasks(ui: &mut Ui, st: &State, out: &str, area: R) {
    let t = ui.t;
    let tg = st.set.num("bar_task_gap");
    let list = task_list(st, out);
    let fit = ((area.h + tg) / (VTASK + tg)).max(0) as usize;
    let mut y = area.y;
    for w in list.into_iter().take(fit) {
        let r = R::new(area.x + (area.w - VTASK) / 2, y, VTASK, VTASK);
        let hidden = st.niri.is_min(&w);
        let hov = ui.hover(r);
        let fill = if w.focused { t.selection } else if hov { t.surface_hi } else if hidden { CLEAR } else { t.surface };
        let outline = if w.focused || hov { t.red } else if hidden { t.border.fade(0.6) } else { t.border };
        ui.cv.pxbox(r, &BoxStyle::new(fill, outline).bevel(!hidden).shadow(if w.focused { 2 } else { 0 }));
        if w.focused {
            ui.cv.fill(R::new(r.x + 4, r.bottom() - 3, r.w - 8, 1), t.ember);
        }
        if w.urgent && !w.focused {
            ui.cv.outline(r, t.ember, 1);
        }
        let icon = icons::icon_for(&st.apps, &w.app_id);
        ui.app_icon(&icon, r.x + 5, r.y + 5, 16, if hidden { 0.45 } else { 1.0 });
        ui.zone(r, Act::Task(w.id), Cur::Pointer);
        y += VTASK + tg;
    }
}

/// окна стола на мониторе: на его активном столе и свёрнутые
fn task_list(st: &State, out: &str) -> Vec<crate::niri::Win> {
    let Some(ws) = st.niri.active_ws(out) else { return Vec::new() };
    let mut list = st.niri.windows_on(ws.id);
    list.extend(st.niri.minimized());
    list
}

/// ширина окон стола не больше max: кнопки по taskWidth, теснее — уже, но не меньше MIN_TASK
/// (тогда влезают не все); 0 — окон нет или нет места даже под одну кнопку
const MIN_TASK: i32 = 24;
fn tasks_width(st: &State, out: &str, max: i32) -> i32 {
    let n = task_list(st, out).len() as i32;
    if n == 0 {
        return 0;
    }
    let tg = st.set.num("bar_task_gap");
    let iw = st.set.num("task_width").min((max.saturating_add(tg)) / n - tg).max(MIN_TASK);
    let fit = (max.saturating_add(tg) / (iw + tg)).clamp(0, n);
    if fit == 0 {
        return 0;
    }
    fit * iw + (fit - 1) * tg
}

/// окна стола: активное — в акценте с яркой полоской, свёрнутое — блёклое, со значком
fn tasks(ui: &mut Ui, st: &State, out: &str, area: R) {
    let t = ui.t;
    let list = task_list(st, out);
    if list.is_empty() || area.w <= 0 {
        return;
    }
    let n = list.len() as i32;
    let tg = st.set.num("bar_task_gap");
    let iw = st.set.num("task_width").min((area.w + tg) / n - tg).max(MIN_TASK);
    ui.cv.push_clip(area);
    let mut x = area.x;
    for w in list {
        let r = R::new(x, area.cy() - 12, iw, 24);
        let hidden = st.niri.is_min(&w);
        let hov = ui.hover(r);
        let fill = if w.focused { t.selection } else if hov { t.surface_hi } else if hidden { CLEAR } else { t.surface };
        let outline = if w.focused || hov { t.red } else if hidden { t.border.fade(0.6) } else { t.border };
        ui.cv.pxbox(r, &BoxStyle::new(fill, outline).bevel(!hidden).shadow(if w.focused { 2 } else { 0 }));
        if w.focused {
            ui.cv.fill(R::new(r.x + 4, r.bottom() - 3, r.w - 8, 1), t.ember);
        }
        if w.urgent && !w.focused {
            ui.cv.outline(r, t.ember, 1);
        }
        let mut cx = r.x + 6;
        let icon = icons::icon_for(&st.apps, &w.app_id);
        ui.app_icon(&icon, cx, r.cy() - 8, 16, if hidden { 0.45 } else { 1.0 });
        cx += 22;
        if hidden {
            ui.cv.icon("min", cx, r.cy() - 7, 2, t.text_faint);
            cx += 20;
        }
        let title = if w.title.is_empty() { &w.app_id } else { &w.title };
        let c = if w.focused { t.text_hi } else if hidden { t.text_faint } else { t.text };
        ui.text_in(font::PX, R::new(cx, r.y, r.right() - cx - 6, r.h), title, c);
        ui.zone(r, Act::Task(w.id), Cur::Pointer);
        x += iw + tg;
    }
    ui.cv.pop_clip();
}

impl App {
    /// поверхность бара на мониторе
    pub fn mk_bar(&mut self, out: &str) {
        use smithay_client_toolkit::shell::wlr_layer::{KeyboardInteractivity, Layer};
        let p = self.bar_place(out);
        let id = self.mk(SKind::Bar, out, Layer::Top, "yami-bar", p.anchor, p.size, p.margin, H, KeyboardInteractivity::None);
        if let Some(s) = self.surfs.get_mut(&id) {
            s.anchor_side = p.key;
            // mk считает угол по отступам сверху и слева, а у нижнего и правого бара они с другой стороны
            s.margin = p.origin;
            s.applied = p.origin;
        }
    }

    /// якорь, отступы и левый верхний угол поверхности бара на мониторе
    fn bar_place(&self, out: &str) -> BarPlace {
        use smithay_client_toolkit::shell::wlr_layer::Anchor;
        let (ow, oh) = self.output_size(out);
        let v = vert(&self.st.set);
        let (m, side, th) = geo(&self.st.set, if v { oh } else { ow });
        let key = format!("{} {m} {side} {th} {ow} {oh}", self.st.set.bar_pos);
        let (anchor, margin, size, origin) = match self.st.set.bar_pos.as_str() {
            "bottom" => (Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT, (0, side, m, side), (0, th), (side, oh - m - th)),
            "left" => (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (side, 0, side, m), (th, 0), (m, side)),
            "right" => (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (side, m, side, 0), (th, 0), (ow - m - th, side)),
            _ => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (m, side, 0, side), (0, th), (side, m)),
        };
        BarPlace { anchor, margin, size, origin, key }
    }

    /// форма бара сменилась (настройки, размер монитора): поверхность меняется на месте, не
    /// пересоздаётся — у niri бар остаётся первым и не уступает место доку сбоку
    pub fn sync_bar(&mut self) {
        use smithay_client_toolkit::shell::WaylandSurface;
        let mut moved = false;
        for id in self.find(&SKind::Bar) {
            let out = self.surfs.get(&id).map(|s| s.output.clone()).unwrap_or_default();
            let p = self.bar_place(&out);
            let Some(s) = self.surfs.get_mut(&id) else { continue };
            if s.anchor_side == p.key {
                continue;
            }
            s.anchor_side = p.key;
            s.layer.set_anchor(p.anchor);
            s.layer.set_size(p.size.0 as u32, p.size.1 as u32);
            s.layer.set_margin(p.margin.0, p.margin.1, p.margin.2, p.margin.3);
            s.layer.set_exclusive_zone(H);
            s.layer.commit();
            s.margin = p.origin;
            s.applied = p.origin;
            s.dirty = true;
            moved = true;
        }
        if moved {
            // всё, что встаёт рядом с баром, — на новое место
            self.close_popup();
            self.close_menu();
            self.close_kind(&SKind::Catcher);
            self.sync_dock();
            self.sync_widgets();
            self.sync_titles();
        }
    }

    pub fn bar_on(&self, out: &str) -> bool {
        shown_on(&self.st.set.bar_outputs, &self.output_names(), out)
    }

    /// бары по barOutputs: лишние убрать, недостающие создать
    pub fn sync_bar_outputs(&mut self) {
        let names = self.output_names();
        for n in names {
            let have = self.find_on(&SKind::Bar, &n);
            let want = !self.st.bar_hidden && self.bar_on(&n);
            match (have, want) {
                (Some(id), false) => {
                    self.destroy(id);
                    self.close_popup();
                    self.close_menu();
                }
                (None, true) => self.mk_bar(&n),
                _ => {}
            }
        }
    }

    /// сколько бар занимает сверху (0 — бара нет или он у другого края): док сверху, виджеты,
    /// заголовки окон и всё, что встаёт под верхний край, — ниже этого
    pub fn bar_bottom(&self, out: &str) -> i32 {
        self.bar_reserve(out).0
    }

    /// место бара у каждого края монитора: (сверху, справа, снизу, слева), как у set_margin
    pub fn bar_reserve(&self, out: &str) -> (i32, i32, i32, i32) {
        if self.st.bar_hidden || !self.bar_on(out) {
            return (0, 0, 0, 0);
        }
        let (ow, oh) = self.output_size(out);
        let s = &self.st.set;
        let span = geo(s, if vert(s) { oh } else { ow }).0 + H;
        match s.bar_pos.as_str() {
            "bottom" => (0, 0, span, 0),
            "left" => (0, 0, 0, span),
            "right" => (0, span, 0, 0),
            _ => (span, 0, 0, 0),
        }
    }

    /// прямоугольник самого бара на мониторе (без тени); None — бара на нём нет
    pub fn bar_rect(&self, out: &str) -> Option<R> {
        if self.st.bar_hidden || !self.bar_on(out) {
            return None;
        }
        let (ow, oh) = self.output_size(out);
        let s = &self.st.set;
        let (m, side, _) = geo(s, if vert(s) { oh } else { ow });
        Some(match s.bar_pos.as_str() {
            "bottom" => R::new(side, oh - m - H, ow - 2 * side, H),
            "left" => R::new(m, side, H, oh - 2 * side),
            "right" => R::new(ow - m - H, side, H, oh - 2 * side),
            _ => R::new(side, m, ow - 2 * side, H),
        })
    }
}

struct BarPlace {
    anchor: smithay_client_toolkit::shell::wlr_layer::Anchor,
    /// сверху, справа, снизу, слева — как у set_margin
    margin: (i32, i32, i32, i32),
    /// размер для set_size: длина вдоль края — 0 (растянут якорями), толщина — своя
    size: (i32, i32),
    origin: (i32, i32),
    key: String,
}

/// бар на экране out (barOutputs = list, present — экраны сейчас): список пуст — везде; ни одного
/// экрана из списка нет (ноутбук без своего монитора) — тоже везде, иначе бара не осталось бы нигде
pub fn shown_on(list: &[String], present: &[String], out: &str) -> bool {
    list.is_empty() || list.iter().any(|o| o == out) || !present.iter().any(|n| list.contains(n))
}
