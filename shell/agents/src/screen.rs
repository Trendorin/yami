// Окно «что показать» для показа экрана (OBS, Discord, браузеры): бэкенд портала (screencast.rs)
// спрашивает и ждёт ответ: «monitor DP-1», «window 42» или None (отмена). Мониторы — карточками,
// окна — списком со значками. Список берётся у niri: только он отдаёт показ экрана этому бэкенду.

use crate::app::{Act, App, Ui};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, Modifiers};
use std::sync::mpsc::Sender;
use yami_px::font::PX;
use yami_px::gfx::{BoxStyle, R};
use yami_px::ui::{Cur, Field, Tone};

const W: i32 = 560;
/// окон в списке не больше — дальше колесо
const ROWS: usize = 8;

pub struct Item {
    /// «monitor DP-1» / «window 42» — то, что уйдёт порталу
    pub reply: String,
    pub monitor: bool,
    pub title: String,
    pub sub: String,
    pub icon: String,
    /// монитор: размер для мини-схемы
    pub size: (i32, i32),
}

pub struct Pick {
    pub reply: Sender<Option<String>>,
    pub items: Vec<Item>,
    pub sel: usize,
    pub scroll: usize,
}

pub fn size(p: &Pick) -> (i32, i32) {
    let mons = p.items.iter().any(|i| i.monitor);
    let wins = p.items.iter().filter(|i| !i.monitor).count().min(ROWS) as i32;
    (W, 44 + if mons { 30 + 96 + 12 } else { 0 } + if wins > 0 { 30 + wins * 32 + 8 } else { 0 } + 52)
}

/// заголовок раздела с полоской под ним
fn title(ui: &mut Ui, x: i32, y: i32, w: i32, s: &str) -> i32 {
    let t = ui.t;
    ui.cv.fill(R::new(x, y + 7, 6, 6), t.red);
    ui.text_in(PX, R::new(x + 14, y, w - 14, 20), s, t.text);
    ui.cv.fill(R::new(x, y + 24, w, 1), t.border);
    y + 34
}

/// для `shot`
pub fn mock() -> Pick {
    let (reply, _) = std::sync::mpsc::channel();
    let w = |t: &str, a: &str, id: u32| Item { reply: format!("window {id}"), monitor: false, title: t.into(), sub: a.into(), icon: a.into(), size: (0, 0) };
    Pick { reply, items: vec![Item { reply: "monitor DP-1".into(), monitor: true, title: "DP-1".into(), sub: "1920×1080".into(), icon: String::new(), size: (1920, 1080) }, w("yami: a terminal", "kitty", 1), w("Mozilla Firefox", "firefox", 2)], sel: 0, scroll: 0 }
}

/// правка поля пароля; Ctrl+A/U/V — и в русской раскладке (по положению клавиши)
pub fn edit_field(f: &mut Field, e: &KeyEvent, mods: &Modifiers) {
    match e.keysym {
        Keysym::BackSpace => f.backspace(mods.ctrl),
        Keysym::Delete => f.delete(),
        Keysym::Left => f.left(),
        Keysym::Right => f.right(),
        Keysym::Home => f.home(),
        Keysym::End => f.end(),
        _ if mods.ctrl => match e.raw_code {
            22 => f.clear(),
            30 => f.select_all(),
            47 => {
                if let Some(t) = yami_px::ui::paste() {
                    f.insert(&t);
                }
            }
            _ => {}
        },
        _ => {
            if let Some(s) = e.utf8.as_deref().filter(|s| !s.is_empty() && !s.chars().any(|c| c.is_control())) {
                f.insert(s);
            }
        }
    }
}

fn niri_json(what: &str) -> serde_json::Value {
    std::process::Command::new("niri").args(["msg", "-j", what]).output().ok().and_then(|o| serde_json::from_slice(&o.stdout).ok()).unwrap_or(serde_json::Value::Null)
}

pub fn draw(ui: &mut Ui, p: &Pick, w: i32, h: i32) {
    let t = ui.t;
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, true);
    ui.header(R::new(1, 1, fr.w - 2, 30), "screen", "что показать", "");
    let x = 16;
    let iw = fr.w - 32;
    let mut y = 42;
    let mons: Vec<(usize, &Item)> = p.items.iter().enumerate().filter(|(_, i)| i.monitor).collect();
    if !mons.is_empty() {
        y = title(ui, x, y, iw, "экраны");
        let cw = ((iw - (mons.len() as i32 - 1) * 10) / mons.len() as i32).min(170);
        for (k, (i, it)) in mons.iter().enumerate() {
            let r = R::new(x + k as i32 * (cw + 10), y, cw, 92);
            let sel = p.sel == *i;
            let hov = ui.hover(r);
            ui.cv.pxbox(r, &BoxStyle::new(if sel { t.selection } else { t.surface }, if sel { t.ember } else if hov { t.red } else { t.border }).bevel(true).shadow(2));
            // мини-монитор в пропорциях экрана
            let (mw, mh) = it.size;
            let k2 = (46.0 / mh.max(1) as f32).min((cw - 24) as f32 / mw.max(1) as f32);
            let (sw, sh) = (((mw as f32 * k2) as i32).max(8), ((mh as f32 * k2) as i32).max(6));
            let m = R::new(r.x + (r.w - sw) / 2, r.y + 8, sw, sh);
            ui.cv.pxbox(m, &BoxStyle::new(t.code_bg, if sel { t.ember } else { t.border }));
            ui.cv.fill(R::new(m.x + m.w / 2 - 6, m.bottom(), 12, 3), t.border);
            ui.cv.text_center(ui.f, PX, R::new(r.x, r.y + 58, r.w, 16), &it.title, t.text);
            ui.cv.text_center(ui.f, PX, R::new(r.x, r.y + 74, r.w, 16), &it.sub, if sel { t.text_dim } else { t.text_faint });
            ui.zone(r, Act::ScreenSel(*i), Cur::Pointer);
        }
        y += 104;
    }
    let wins: Vec<(usize, &Item)> = p.items.iter().enumerate().filter(|(_, i)| !i.monitor).collect();
    if !wins.is_empty() {
        let more = if wins.len() > ROWS { crate::tf!("{} — колесо листает", "{} — scroll for more", "{} — role para ver mais", wins.len()) } else { String::new() };
        y = title(ui, x, y, iw, "окна");
        if !more.is_empty() {
            ui.text_right(PX, R::new(x, y - 34, iw, 20), &more, t.text_faint);
        }
        for (i, it) in wins.iter().skip(p.scroll).take(ROWS).map(|(i, it)| (*i, *it)) {
            let r = R::new(x, y, iw, 30);
            let sel = p.sel == i;
            ui.button(r, "", "", Tone::Ghost, sel, true, Act::ScreenSel(i));
            ui.app_icon(&it.icon, r.x + 8, r.y + 7, 16, 1.0);
            ui.cv.text_in(ui.f, PX, R::new(r.x + 32, r.y, r.w - 200, r.h), &it.title, if sel { t.text } else { t.text_dim });
            ui.cv.text_right(ui.f, PX, R::new(r.x, r.y, r.w - 10, r.h), &it.sub, t.text_faint);
            y += 32;
        }
    }
    let by = fr.h - 40;
    ui.button(R::new(fr.w - 16 - 260, by, 124, 28), "отмена", "close", Tone::Plain, false, true, Act::ScreenCancel);
    ui.button(R::new(fr.w - 16 - 130, by, 130, 28), "показать", "check", Tone::Primary, false, !p.items.is_empty(), Act::ScreenOk);
    ui.text_in(PX, R::new(x, by, fr.w - 300, 28), "enter — показать · esc — отмена", t.text_faint);
}

impl App {
    /// запрос от портала: собрать мониторы и окна, показать окно выбора
    pub fn screen_ask(&mut self, types: u32, reply: Sender<Option<String>>) {
        if let Some(old) = self.pick.take() {
            let _ = old.reply.send(None);
        }
        let mut items = vec![];
        if types & 1 != 0 || types == 0 {
            let outs = niri_json("outputs");
            for (name, o) in outs.as_object().into_iter().flatten() {
                let Some(l) = o.get("logical").filter(|l| !l.is_null()) else { continue };
                let (w, h) = (l["width"].as_i64().unwrap_or(0) as i32, l["height"].as_i64().unwrap_or(0) as i32);
                items.push(Item { reply: format!("monitor {name}"), monitor: true, title: name.clone(), sub: format!("{w}×{h}"), icon: String::new(), size: (w, h) });
            }
        }
        if types & 2 != 0 {
            if self.apps.is_empty() {
                self.apps = yami_px::icons::load_apps();
            }
            for w in niri_json("windows").as_array().into_iter().flatten() {
                let Some(id) = w["id"].as_u64() else { continue };
                let app_id = w["app_id"].as_str().unwrap_or("").to_string();
                let title = w["title"].as_str().filter(|t| !t.is_empty()).unwrap_or(&app_id).to_string();
                let icon = yami_px::icons::icon_for(&self.apps, &app_id);
                items.push(Item { reply: format!("window {id}"), monitor: false, title, sub: app_id, icon, size: (0, 0) });
            }
        }
        // по умолчанию — экран в фокусе
        let focus = niri_json("focused-output");
        let focus = focus["name"].as_str().unwrap_or("");
        let sel = items.iter().position(|i| i.monitor && i.title == focus).unwrap_or(0);
        self.pick = Some(Pick { reply, items, sel, scroll: 0 });
        self.sync_panel();
        self.dirty();
    }

    /// ответ порталу и закрыть окно (false — отмена)
    pub fn screen_reply(&mut self, ok: bool) {
        let Some(p) = self.pick.take() else { return };
        let _ = p.reply.send(p.items.get(p.sel).filter(|_| ok).map(|i| i.reply.clone()));
        self.sync_panel();
    }

    pub fn screen_click(&mut self, a: &Act, double: bool) {
        match a {
            Act::ScreenSel(i) => {
                if let Some(p) = self.pick.as_mut() {
                    p.sel = *i;
                }
                if double {
                    self.screen_reply(true);
                    return;
                }
            }
            Act::ScreenOk => self.screen_reply(true),
            Act::ScreenCancel => self.screen_reply(false),
            _ => {}
        }
        self.dirty();
    }

    pub fn screen_wheel(&mut self, up: bool) {
        if let Some(p) = self.pick.as_mut() {
            let n = p.items.iter().filter(|i| !i.monitor).count();
            p.scroll = if up { p.scroll.saturating_sub(1) } else { (p.scroll + 1).min(n.saturating_sub(ROWS)) };
        }
        self.dirty();
    }

    pub fn screen_key(&mut self, e: &KeyEvent) {
        let Some(p) = self.pick.as_mut() else { return };
        let n = p.items.len().max(1);
        match e.keysym {
            Keysym::Escape => return self.screen_reply(false),
            Keysym::Return | Keysym::KP_Enter => return self.screen_reply(true),
            Keysym::Down | Keysym::Right | Keysym::Tab => p.sel = (p.sel + 1) % n,
            Keysym::Up | Keysym::Left | Keysym::ISO_Left_Tab => p.sel = (p.sel + n - 1) % n,
            _ => {}
        }
        // выбранное окно — в видимой части списка
        let first_win = p.items.iter().position(|i| !i.monitor).unwrap_or(p.items.len());
        if p.sel >= first_win {
            let k = p.sel - first_win;
            if k < p.scroll {
                p.scroll = k;
            } else if k >= p.scroll + ROWS {
                p.scroll = k + 1 - ROWS;
            }
        }
        self.dirty();
    }
}
