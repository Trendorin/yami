// Мастер первой настройки: страницы, их состояние и отрисовка. Язык и вид применяются сразу:
// весь мастер перекрашивается и меняет стиль, пока человек выбирает. Ставит не он, а install.sh
// (run.rs): здесь только выбор.
// i18n: inline pairs

use crate::run::Ev;
use std::time::Instant;
use yami_px::font::{self, Face, PX, PX24};
use yami_px::gfx::{BoxStyle, C, R};
use yami_px::look::Look;
use yami_px::theme::{CLEAR, PRESETS};
use yami_px::ui::{Cur, Field};

#[derive(Clone, PartialEq, Debug)]
pub enum Act {
    Block,
    Lang(&'static str),
    Preset(bool),
    Accent(u32),
    Light(bool),
    Module(usize),
    Restyle(usize),
    Pass,
    Back,
    Next,
    Skip,
    Finish,
    Retry,
}

impl yami_px::ui::Action for Act {
    fn block() -> Self {
        Act::Block
    }
}

pub type Ui<'a, 'b> = yami_px::ui::Ui<'a, 'b, Act>;

/// текст на языке мастера
pub fn l<'a>(ru: &'a str, en: &'a str, pt: &'a str) -> &'a str {
    if yami_px::i18n::pt_br() {
        pt
    } else if yami_px::i18n::translated() {
        en
    } else {
        ru
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Hello,
    Look,
    Modules,
    Restyle,
    Confirm,
    Install,
    Done,
}

/// страницы, по которым идут кнопками «назад» и «дальше»
const STEPS: [Page; 5] = [Page::Hello, Page::Look, Page::Modules, Page::Restyle, Page::Confirm];

pub struct Module {
    pub id: &'static str,
    pub name: &'static str,
    pub group: u8,
    pub on: bool,
    pub ru: &'static str,
    pub en: &'static str,
    pub pt: &'static str,
}

const fn m(id: &'static str, name: &'static str, group: u8, on: bool, ru: &'static str, en: &'static str, pt: &'static str) -> Module {
    Module { id, name, group, on, ru, en, pt }
}

pub fn modules() -> Vec<Module> {
    vec![
        m("files", "yami-files", 0, false, "файлы, диалог выбора во всех программах, флешки и телефоны", "files, the file dialog of every app, drives and phones", "arquivos, o diálogo de todos os apps, pendrives e telefones"),
        m("view", "yami-view", 0, false, "картинки: мгновенно, с быстрыми правками", "pictures: instant, with quick edits", "imagens: instantâneo, com edições rápidas"),
        m("play", "yami-play", 0, false, "видео и музыка на libmpv", "video and music on libmpv", "vídeo e música com libmpv"),
        m("shot", "yami-shot", 0, false, "скриншоты: разметка, текст и QR со снимка", "screenshots: markup, text and QR from a shot", "capturas: anotações, texto e QR"),
        m("agents", "yami-agents", 1, false, "окна паролей, показ экрана, автоподключение", "password windows, screen sharing, automount", "senhas, compartilhar a tela, montagem automática"),
        m("login", "yami-greet", 1, false, "свой экран входа вместо чужого", "our own login screen", "nossa própria tela de login"),
        m("games", "games", 2, false, "GameMode и MangoHud в палитре", "GameMode and MangoHud in the palette", "GameMode e MangoHud na paleta"),
        m("fork", "yami-niri", 2, false, "наш niri из исходников: 5–25 минут сборки", "our niri from source: a 5–25 minute build", "nosso niri do código-fonte: 5–25 minutos"),
    ]
}

/// что можно перекрасить: id как у `install.sh --restyle`
pub const RESTYLE: [(&str, &str); 11] = [("gtk", "GTK"), ("qt", "Qt / KDE"), ("cursor", "cursor"), ("kitty", "kitty"), ("fastfetch", "fastfetch"), ("btop", "btop"), ("firefox", "Firefox"), ("mangohud", "MangoHud"), ("foot", "foot"), ("cava", "cava"), ("starship", "starship")];

pub struct W {
    pub page: Page,
    pub reveal: Instant,
    pub look: Look,
    pub lang: String,
    pub plain: bool,
    pub accent: u32,
    pub light: bool,
    pub mods: Vec<Module>,
    pub restyle: [bool; 11],
    pub field: Field,
    /// sudo спрашивать не будет (NOPASSWD), или его нет
    pub no_password: bool,
    /// sudo нет (doas): окну пароль не передать, пакеты — одной командой потом
    pub no_root: bool,
    /// другой менеджер входа, который заменит наш экран входа
    pub dm: String,
    /// есть Steam: показать строку про игры
    pub steam: bool,
    pub lines: Vec<String>,
    pub step: Option<(u32, u32, String)>,
    pub failed: Option<String>,
    pub running: bool,
    pub wrong: bool,
    /// стиль на момент открытия: только смена стиля трогает проявление, значки и матовость
    pub was_plain: bool,
}

fn read_json(p: &std::path::Path) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()
}

impl W {
    /// то, что уже стоит и выбрано, — из записи об установке и настроек шелла
    pub fn fill_from_system(&mut self) {
        let first = yami_px::paths::state_dir().join("first-run").exists();
        self.dm = other_dm();
        self.steam = has_steam();
        if first {
            // чужой экран входа по умолчанию не заменяем; игры — только если есть Steam
            for m in self.mods.iter_mut() {
                match m.id {
                    "login" => m.on = self.dm.is_empty(),
                    "games" => m.on = self.steam,
                    _ => {}
                }
            }
        }
        if let Some(list) = read_json(&yami_px::paths::state_dir().join("install.json")).and_then(|v| v["components"].as_str().map(str::to_string)) {
            if !first {
                for m in self.mods.iter_mut() {
                    m.on = list.split_whitespace().any(|c| c == m.id);
                }
            }
        }
        let Some(s) = read_json(&yami_px::paths::shell_settings()) else { return };
        self.plain = s["preset"].as_str() == Some("plain");
        self.was_plain = self.plain;
        self.light = s["light"].as_bool().unwrap_or(false);
        let accent = s["accent"].as_str().and_then(yami_px::theme::parse_hex);
        let preset = PRESETS.iter().find(|p| Some(p.id) == s["palette"].as_str()).map(|p| p.accent);
        self.accent = accent.or(preset).unwrap_or(self.accent);
        if let Some(a) = s["themeApps"].as_array().filter(|_| !first) {
            for (i, (id, _)) in RESTYLE.iter().enumerate() {
                self.restyle[i] = *id == "cursor" || a.iter().any(|x| x.as_str() == Some(id));
            }
        }
    }

    pub fn new(look: Look) -> W {
        W {
            page: Page::Hello,
            reveal: Instant::now(),
            // язык системы сразу отмечен на первой странице
            lang: if yami_px::i18n::pt_br() { "pt-BR".into() } else if yami_px::i18n::translated() { "en".into() } else { "ru".into() },
            plain: false,
            accent: PRESETS.iter().find(|p| p.id == "ash").map_or(PRESETS[0].accent, |p| p.accent),
            light: false,
            look,
            mods: modules(),
            restyle: [true, true, true, true, true, true, true, false, false, false, false],
            field: Field::default(),
            no_password: false,
            no_root: false,
            dm: String::new(),
            steam: false,
            lines: vec![],
            step: None,
            failed: None,
            running: false,
            wrong: false,
            was_plain: false,
        }
    }

    /// настройки вида, которые мастер выбрал: для него самого и для файла настроек шелла
    pub fn style_json(&self) -> serde_json::Value {
        // акцент из пресетов — по имени: так он отмечен и в настройках шелла
        let (palette, accent) = match PRESETS.iter().find(|p| p.accent == self.accent) {
            Some(p) => (p.id.to_string(), String::new()),
            None => (String::new(), format!("#{:06x}", self.accent)),
        };
        serde_json::json!({
            "preset": if self.plain { "plain" } else { "yami" },
            "palette": palette,
            "accent": accent,
            "light": self.light,
            "lang": self.lang,
        })
    }

    /// сверх style_json — что идёт в настройки шелла, только если стиль сменили
    pub fn settings_json(&self) -> serde_json::Value {
        let mut v = self.style_json();
        if self.plain != self.was_plain {
            if let Some(o) = v.as_object_mut() {
                o.insert("reveal".into(), if self.plain { "fade" } else { "dither" }.into());
                o.insert("iconPixel".into(), (!self.plain).into());
                o.insert("glass".into(), if self.plain { 1.0 } else { 0.55 }.into());
            }
        }
        v
    }

    pub fn turn(&mut self, p: Page) {
        self.page = p;
        self.reveal = Instant::now();
    }

    pub fn on_event(&mut self, e: Ev) {
        match e {
            Ev::Step(k, n, t) => self.step = Some((k, n, t)),
            Ev::Line(s) => {
                self.lines.push(s);
                let over = self.lines.len().saturating_sub(12);
                self.lines.drain(..over);
            }
            Ev::Done(Ok(())) => {
                self.running = false;
                self.turn(Page::Done);
            }
            Ev::Done(Err(why)) => {
                self.running = false;
                self.failed = Some(why);
            }
            Ev::WrongPassword => {
                self.running = false;
                self.wrong = true;
                self.turn(Page::Confirm);
            }
        }
    }
}

/// менеджер входа, который стоит вместо нашего (systemd); свой не считается
fn other_dm() -> String {
    let Some(l) = std::fs::read_link("/etc/systemd/system/display-manager.service").ok() else { return String::new() };
    let name = l.file_stem().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ours = (name == "greetd" && std::fs::read_to_string("/etc/greetd/config.toml").is_ok_and(|t| t.contains("yami/greet-session"))) || (name == "sddm" && std::path::Path::new("/usr/share/sddm/themes/yami").is_dir());
    if ours { String::new() } else { name }
}

fn has_steam() -> bool {
    let h = yami_px::paths::home();
    [".local/share/Steam", ".steam", ".var/app/com.valvesoftware.Steam"].iter().any(|d| h.join(d).is_dir()) || std::path::Path::new("/usr/bin/steam").exists()
}

fn step_index(p: Page) -> usize {
    STEPS.iter().position(|s| *s == p).unwrap_or(STEPS.len())
}

pub fn next_of(p: Page) -> Option<Page> {
    STEPS.get(step_index(p) + 1).copied()
}

pub fn prev_of(p: Page) -> Option<Page> {
    step_index(p).checked_sub(1).and_then(|i| STEPS.get(i)).copied()
}

/// размеры страницы: на большом экране всё крупнее, шрифт — целые кратные своей сетки
#[derive(Clone, Copy)]
struct M {
    text: Face,
    small: Face,
    title: Face,
    hero: Face,
    logo: u16,
    /// клетка иконки 7×7
    cell: i32,
    btn: i32,
    row: i32,
    gap: i32,
    pad: i32,
    check: i32,
    swatch: i32,
    card: i32,
}

fn metrics(sw: i32, sh: i32) -> M {
    if sw >= 1600 && sh >= 900 {
        M { text: PX24, small: PX, title: font::px(48), hero: font::px(64), logo: 64, cell: 3, btn: 56, row: 84, gap: 12, pad: 72, check: 28, swatch: 40, card: 340 }
    } else {
        M { text: PX, small: PX, title: font::px(32), hero: font::px(48), logo: 48, cell: 2, btn: 40, row: 58, gap: 8, pad: 44, check: 20, swatch: 28, card: 220 }
    }
}

/// вся страница: слева знак и шаги, справа содержимое и кнопки на одном месте
pub fn draw(ui: &mut Ui, w: &W, sw: i32, sh: i32) {
    let t = ui.t;
    let m = metrics(sw, sh);
    ui.cv.fill(R::new(0, 0, sw, sh), t.bg);
    let rail_w = if sw < 960 { 0 } else { (sw * 24 / 100).clamp(300, 460) };
    if rail_w > 0 {
        rail(ui, w, m, R::new(0, 0, rail_w, sh));
    }
    let area = R::new(rail_w, 0, sw - rail_w, sh);
    let inner = (area.w - 2 * m.pad).min(1180);
    let b = R::new(area.x + (area.w - inner) / 2, m.pad, inner, sh - 2 * m.pad);
    match w.page {
        Page::Hello => hello(ui, w, m, b),
        Page::Look => look_page(ui, w, m, b),
        Page::Modules => modules_page(ui, w, m, b),
        Page::Restyle => restyle_page(ui, w, m, b),
        Page::Confirm => confirm(ui, w, m, b),
        Page::Install => install(ui, w, m, b),
        Page::Done => done(ui, w, m, b),
    }
    let ms = w.reveal.elapsed().as_millis() as f32;
    let dur = w.look.dur(260.0);
    if dur > 0.0 && ms < dur {
        ui.cv.dissolve_rect(area, ms / dur, 3);
        ui.anim = true;
    }
}

/// левая панель: знак, шаги, большой 闇 у нижнего края (только у пиксельного вида)
fn rail(ui: &mut Ui, w: &W, m: M, r: R) {
    let t = ui.t;
    ui.cv.fill(r, t.panel);
    ui.cv.fill(R::new(r.right() - 1, r.y, 1, r.h), t.border);
    if !w.plain {
        ui.cv.push_clip(r);
        ui.text(font::jp(240), r.right() - 216, r.bottom() - 232, "闇", t.red.alpha(0.10)); // i18n-ok
        ui.cv.pop_clip();
    }
    let x = r.x + m.pad / 2 + 12;
    let mut y = m.pad;
    let logo = font::px(m.logo);
    ui.cv.text_in(ui.f, logo, R::new(x, y, r.w - 2 * (x - r.x), m.logo as i32 + 8), "yami", t.text_hi);
    y += m.logo as i32 + 8;
    ui.cv.text_in(ui.f, m.small, R::new(x, y, r.w - 2 * (x - r.x), 22), l("первая настройка", "first-run setup", "configuração inicial"), t.text_dim);
    y += 22 + m.pad;
    let names = [l("Язык", "Language", "Idioma"), l("Вид", "Look", "Aparência"), l("Модули", "Modules", "Módulos"), l("Цвета программ", "App colours", "Cores dos apps"), l("Установка", "Install", "Instalar")];
    let cur = match w.page {
        Page::Install => names.len() - 1,
        Page::Done => names.len(),
        p => step_index(p),
    };
    let size = m.check + 8;
    let step = size + m.gap * 3;
    for (i, name) in names.iter().enumerate() {
        let b = R::new(x, y + i as i32 * step, size, size);
        if i + 1 < names.len() {
            ui.cv.fill(R::new(b.x + size / 2 - 1, b.bottom() + 3, 2, step - size - 6), if i < cur { t.red } else { t.border });
        }
        let (fill, line, fg) = if i < cur {
            (t.red, t.red, t.on_accent)
        } else if i == cur {
            (t.selection, t.ember, t.text_hi)
        } else {
            (t.surface, t.border, t.text_dim)
        };
        ui.cv.pxbox(b, &BoxStyle::new(fill, line));
        if i < cur {
            let s = 7 * (m.cell - 1).max(1);
            ui.cv.icon("check", b.x + (size - s) / 2, b.y + (size - s) / 2, (m.cell - 1).max(1), fg);
        } else {
            ui.cv.text_center(ui.f, m.small, b, &(i + 1).to_string(), fg);
        }
        ui.cv.text_in(ui.f, m.text, R::new(b.right() + m.gap + 2, b.y, r.right() - b.right() - m.gap - 12, size), name, if i == cur { t.text_hi } else if i < cur { t.text } else { t.text_dim });
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Plain,
    Ghost,
}

/// кнопка размера страницы; по виду как ui.button, но шрифт и иконка растут вместе с экраном
#[allow(clippy::too_many_arguments)]
fn btn(ui: &mut Ui, m: M, r: R, text: &str, icon: &str, kind: Kind, on: bool, act: Act) {
    let t = ui.t;
    let hov = ui.hover(r);
    let down = hov && ui.down;
    let h = if hov { 1.0 } else { 0.0 };
    let (fill, line, fg) = match kind {
        Kind::Primary => (t.red.lerp(t.ember, h * 0.45), CLEAR, t.on_accent),
        Kind::Plain if on => (t.selection, t.ember, t.text_hi),
        Kind::Plain => (t.surface.lerp(t.surface_hi, h), t.border.lerp(t.red, h), t.text),
        Kind::Ghost => (t.selection.alpha(0.6 * h), CLEAR, t.text_dim.lerp(t.text_hi, h)),
    };
    let d = i32::from(down);
    if kind != Kind::Ghost || hov {
        let solid = kind != Kind::Ghost;
        ui.cv.pxbox(R::new(r.x + d, r.y + d, r.w, r.h), &BoxStyle::new(fill, line).shadow(if solid { 2 } else { 0 }).bevel(solid).pressed(down));
    }
    let tw = ui.f.width(m.text, text);
    let iw = if icon.is_empty() { 0 } else { 7 * m.cell };
    let gap = if iw > 0 { m.gap + 2 } else { 0 };
    let mut x = r.x + (r.w - tw - iw - gap) / 2 + d;
    if iw > 0 {
        ui.cv.icon(icon, x, r.y + (r.h - iw) / 2 + d, m.cell, fg);
        x += iw + gap;
    }
    ui.cv.text_in(ui.f, m.text, R::new(x, r.y + d, tw + 4, r.h), text, fg);
    ui.zone(r, act, Cur::Pointer);
}

/// строка-выбор на всю ширину: флажок, название, под ним описание
#[allow(clippy::too_many_arguments)]
fn pick(ui: &mut Ui, m: M, r: R, name: &str, desc: &str, on: bool, act: Act) {
    let t = ui.t;
    let hov = ui.hover(r);
    let fill = if on { t.selection.alpha(0.45) } else if hov { t.surface_hi } else { t.surface };
    ui.cv.pxbox(r, &BoxStyle::new(fill, if on { t.ember } else if hov { t.red } else { t.border }));
    let c = R::new(r.x + m.gap + 4, r.y + (r.h - m.check) / 2, m.check, m.check);
    ui.cv.pxbox(c, &BoxStyle::new(if on { t.red } else { t.code_bg }, if on { t.red } else { t.border }));
    if on {
        let cell = (m.check - 6) / 7;
        ui.cv.icon("check", c.x + (m.check - 7 * cell) / 2, c.y + (m.check - 7 * cell) / 2, cell, t.on_accent);
    }
    let x = c.right() + m.gap + 4;
    let w_ = r.right() - x - m.gap;
    if desc.is_empty() {
        ui.cv.text_in(ui.f, m.text, R::new(x, r.y, w_, r.h), name, t.text_hi);
    } else {
        let line = ui.f.line(m.text);
        let small = ui.f.line(m.small);
        let top = r.y + (r.h - line - small - 4) / 2;
        ui.cv.text_in(ui.f, m.text, R::new(x, top, w_, line), name, t.text_hi);
        ui.cv.text_in(ui.f, m.small, R::new(x, top + line + 4, w_, small), desc, t.text_dim);
    }
    ui.zone(r, act, Cur::Pointer);
}

/// поле пароля размера страницы (как ui.field: звёзды, курсор всегда виден)
fn pass_field(ui: &mut Ui, m: M, r: R, f: &Field, placeholder: &str) {
    let t = ui.t;
    ui.cv.pxbox(r, &BoxStyle::new(t.code_bg, t.ember));
    let inner = R::new(r.x + 14, r.y, r.w - 28, r.h);
    let shown = "■".repeat(f.text.chars().count());
    let before: String = shown.chars().take(f.cur).collect();
    let bw = ui.f.width(m.text, &before);
    let shift = (bw - (inner.w - 12)).max(0);
    let lh = ui.f.line(m.text);
    ui.cv.push_clip(inner);
    if shown.is_empty() {
        ui.cv.text_in(ui.f, m.text, R::new(inner.x + 16, inner.y, inner.w - 16, inner.h), placeholder, t.text_dim);
    } else {
        ui.cv.text(ui.f, m.text, inner.x - shift, r.y + (r.h - lh) / 2, &shown, t.text);
    }
    if ui.blink {
        ui.cv.fill(R::new(inner.x - shift + bw, r.y + (r.h - lh) / 2, m.cell * 4, lh), t.ember);
    }
    ui.cv.pop_clip();
    ui.zone(r, Act::Pass, Cur::Text);
}

/// заголовок страницы: номер шага, название, одна мысль под ним; возвращает, где начать содержимое
fn head(ui: &mut Ui, m: M, b: R, n: usize, title: &str, sub: &str) -> i32 {
    let t = ui.t;
    let mut y = b.y;
    if n > 0 {
        ui.cv.text_in(ui.f, m.small, R::new(b.x, y, b.w, 20), &format!("{} {n} / {}", l("шаг", "step", "etapa"), STEPS.len()), t.red);
        y += 28;
    }
    let tl = ui.f.line(m.title);
    ui.cv.text_in(ui.f, m.title, R::new(b.x, y, b.w, tl + 4), title, t.text_hi);
    y += tl + 8 + m.gap;
    let sl = ui.f.line(m.text);
    ui.cv.text_wrap(ui.f, m.text, R::new(b.x, y, b.w, 2 * sl + 8), sub, t.text_dim, 2);
    y + 2 * sl + 8 + m.gap * 2
}

/// кнопки внизу, всегда на одном месте
fn nav(ui: &mut Ui, w: &W, m: M, b: R, next: &str, icon: &str) {
    let y = b.bottom() - m.btn;
    let bw = m.btn * 4;
    if prev_of(w.page).is_some() {
        btn(ui, m, R::new(b.x, y, bw * 3 / 4, m.btn), l("назад", "back", "voltar"), "left", Kind::Plain, false, Act::Back);
    }
    btn(ui, m, R::new(b.right() - bw, y, bw, m.btn), next, icon, Kind::Primary, false, Act::Next);
}

/// по центру места между верхом и кнопками
fn middle(m: M, b: R, top: i32, h: i32) -> i32 {
    // под заголовком — ближе к нему, на страницах без заголовка — ровно по центру
    let div = if top > b.y { 4 } else { 2 };
    top + ((b.bottom() - m.btn - m.gap * 2 - top - h) / div).max(0)
}

fn hello(ui: &mut Ui, w: &W, m: M, b: R) {
    let t = ui.t;
    let hl = ui.f.line(m.hero);
    let sl = ui.f.line(m.text);
    let tile = m.btn * 2 + m.gap;
    let total = hl + 12 + 3 * sl + 16 + m.gap * 4 + 24 + tile;
    let mut y = middle(m, b, b.y, total);
    ui.cv.text_in(ui.f, m.hero, R::new(b.x, y, b.w, hl + 4), l("Добро пожаловать", "Welcome", "Bem-vindo"), t.text_hi);
    y += hl + 12;
    ui.cv.text_wrap(ui.f, m.text, R::new(b.x, y, b.w.min(sl * 44), 3 * sl + 16), l("Рабочий стол уже работает. Пара вопросов, и мастер поставит только то, что ты выберешь. Всё можно поменять потом в настройках.", "The desktop is already running. A few questions, and the setup installs only what you pick. Everything can be changed later in the settings.", "A área de trabalho já está funcionando. Algumas perguntas, e a configuração instala só o que você escolher. Tudo pode ser alterado depois nas configurações."), t.text_dim, 3);
    y += 3 * sl + 16 + m.gap * 4;
    ui.cv.text_in(ui.f, m.small, R::new(b.x, y, b.w, 20), l("язык", "language", "idioma"), t.text_dim);
    y += 24;
    let bw = (b.w - 2 * m.gap) / 3;
    for (i, (id, code, name)) in [("ru", "RU", "Русский"), ("en", "EN", "English"), ("pt-BR", "PT", "Português")].into_iter().enumerate() { // i18n-ok
        let on = yami_px::i18n::normalize(&w.lang) == id;
        let r = R::new(b.x + i as i32 * (bw + m.gap), y, bw, tile);
        let hov = ui.hover(r);
        ui.cv.pxbox(r, &BoxStyle::new(if on { t.selection.alpha(0.45) } else if hov { t.surface_hi } else { t.surface }, if on { t.ember } else if hov { t.red } else { t.border }));
        ui.cv.text_in(ui.f, m.title, R::new(r.x + m.gap * 2, r.y + 8, r.w - m.gap * 4, r.h / 2), code, if on { t.text_hi } else { t.text_dim });
        ui.cv.text_in(ui.f, m.text, R::new(r.x + m.gap * 2, r.y + r.h / 2 + 2, r.w - m.gap * 4, r.h / 2 - 8), name, t.text);
        ui.zone(r, Act::Lang(id), Cur::Pointer);
    }
    let bottom = b.bottom() - m.btn;
    let sw = ui.f.width(m.text, l("пропустить настройку", "skip the setup", "pular a configuração")) + m.gap * 4;
    btn(ui, m, R::new(b.x, bottom, sw, m.btn), l("пропустить настройку", "skip the setup", "pular a configuração"), "", Kind::Ghost, false, Act::Skip);
    nav(ui, w, m, b, l("начать", "start", "começar"), "right");
}

/// миниатюра вида: окно с панелью и кнопками, нарисованное этим стилем
#[allow(clippy::too_many_arguments)]
fn look_card(ui: &mut Ui, m: M, r: R, title: &str, sub: &str, plain: bool, on: bool) {
    let t = ui.t;
    let was = ui.cv.style;
    let style = if plain { yami_px::style::Style::PLAIN } else { yami_px::style::Style::PIXEL };
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if on { t.selection.alpha(0.45) } else { t.surface }, if on { t.ember } else if hov { t.red } else { t.border }));
    let text_h = ui.f.line(m.text) + ui.f.line(m.small) + 12;
    let k = m.cell - 1;
    ui.cv.style = style;
    let win = R::new(r.x + m.gap * 2, r.y + m.gap * 2, r.w - m.gap * 4, r.h - m.gap * 4 - text_h);
    ui.cv.pxbox(win, &BoxStyle::new(t.panel, t.border).shadow(3));
    ui.cv.fill(R::new(win.x + 1, win.y + 22 * k, win.w - 2, 1), t.border);
    for i in 0..3 {
        ui.cv.notched(R::new(win.x + (10 + i * 14) * k, win.y + 8 * k, 8 * k, 8 * k), 2, if i == 0 { t.red } else { t.dim });
    }
    let (bw, by) = (win.w * 30 / 100, win.y + 22 * k + (win.h - 22 * k) / 4);
    ui.cv.pxbox(R::new(win.x + 12 * k, by, bw, 24 * k), &BoxStyle::new(t.red, t.ember).bevel(true));
    ui.cv.pxbox(R::new(win.x + 12 * k + bw + 10 * k, by, bw, 24 * k), &BoxStyle::new(t.surface_hi, t.border).bevel(true));
    ui.cv.gauge(R::new(win.x + 12 * k, win.bottom() - 18 * k, win.w - 24 * k, 8 * k), 16, 0.62, t.red, t.surface_hi, 2);
    ui.cv.style = was;
    let y = win.bottom() + m.gap;
    let lh = ui.f.line(m.text);
    ui.cv.text_in(ui.f, m.text, R::new(win.x, y, win.w, lh), title, t.text_hi);
    ui.cv.text_in(ui.f, m.small, R::new(win.x, y + lh + 4, win.w, ui.f.line(m.small)), sub, t.text_dim);
    ui.zone(r, Act::Preset(plain), Cur::Pointer);
}

fn look_page(ui: &mut Ui, w: &W, m: M, b: R) {
    let t = ui.t;
    let top = head(ui, m, b, 2, l("Как будет выглядеть", "How it will look", "Como vai ficar"), l("Вид сразу применяется к этому окну. Каждый цвет можно настроить потом.", "The look applies to this window right away. Every colour can be changed later.", "A aparência é aplicada a esta janela na hora. Cada cor pode ser alterada depois."));
    let total = m.card + m.gap * 3 + m.swatch + 8;
    let mut y = middle(m, b, top, total);
    let cw = (b.w - m.gap * 2) / 2;
    look_card(ui, m, R::new(b.x, y, cw, m.card), l("Пиксели", "Pixels", "Pixels"), l("как задумано: фаски, ступени, Tiny5", "the original: notches, steps, Tiny5", "o original: chanfros, degraus, Tiny5"), false, !w.plain);
    look_card(ui, m, R::new(b.x + cw + m.gap * 2, y, cw, m.card), l("Нейтральный", "Neutral", "Neutro"), l("скругления, системный шрифт, мягкие тени", "rounded corners, the system font, soft shadows", "cantos arredondados, fonte do sistema"), true, w.plain);
    y += m.card + m.gap * 3;
    let row = m.swatch + 8;
    let label = l("акцент", "accent", "destaque");
    ui.cv.text_in(ui.f, m.text, R::new(b.x, y, 200, row), label, t.text);
    let mut x = b.x + ui.f.width(m.text, label) + m.gap * 3;
    for p in PRESETS {
        let r = R::new(x, y + 4, m.swatch, m.swatch);
        let c = C::hex(p.accent);
        if w.accent == p.accent {
            ui.cv.outline(R::new(r.x - 4, r.y - 4, r.w + 8, r.h + 8), t.text_hi, 2);
        }
        ui.cv.notched(r, 2, c);
        ui.zone(r, Act::Accent(p.accent), Cur::Pointer);
        x += m.swatch + m.gap + 4;
    }
    let tw = m.btn * 5 / 2;
    btn(ui, m, R::new(b.right() - 2 * tw - m.gap, y + 4, tw, row - 8), l("тёмная", "dark", "escuro"), "", Kind::Plain, !w.light, Act::Light(false));
    btn(ui, m, R::new(b.right() - tw, y + 4, tw, row - 8), l("светлая", "light", "claro"), "", Kind::Plain, w.light, Act::Light(true));
    nav(ui, w, m, b, l("дальше", "next", "avançar"), "right");
}

fn modules_page(ui: &mut Ui, w: &W, m: M, b: R) {
    let t = ui.t;
    let top = head(ui, m, b, 3, l("Что поставить", "What to install", "O que instalar"), l("Ничего не отмечено: выбери только нужное. Шелл работает без любого модуля, менять выбор можно потом: yami setup.", "Nothing is ticked: pick only what you need. The shell runs without any module, and the choice can change later: yami setup.", "Nada está marcado: escolha só o que precisa. O shell funciona sem qualquer módulo, e dá para mudar depois: yami setup."));
    let col = (b.w - m.gap * 2) / 2;
    let titles = [l("программы", "apps", "aplicativos"), l("система", "system", "sistema"), l("дополнительно", "extras", "extras")];
    let shown = |g: u8| w.mods.iter().filter(|x| x.group == g && (x.id != "games" || w.steam)).count() as i32;
    let cap = 24;
    let height = |n: i32| cap + n * (m.row + m.gap);
    let total = height(shown(0)).max(height(shown(1)) + m.gap + height(shown(2)));
    let start = middle(m, b, top, total);
    let mut rows = [start, start];
    for g in 0..3u8 {
        let c = usize::from(g > 0);
        let x = b.x + c as i32 * (col + m.gap * 2);
        ui.cv.text_in(ui.f, m.small, R::new(x, rows[c], col, 20), titles[g as usize], t.text_dim);
        rows[c] += cap;
        for (i, x_) in w.mods.iter().enumerate().filter(|(_, x_)| x_.group == g && (x_.id != "games" || w.steam)) {
            let replaces;
            let desc = if x_.id == "login" && !w.dm.is_empty() {
                replaces = match () {
                    _ if yami_px::i18n::pt_br() => format!("nossa tela de login, no lugar de {}", w.dm),
                    _ if yami_px::i18n::translated() => format!("our own login screen, replaces {}", w.dm),
                    _ => format!("свой экран входа, заменит {}", w.dm), // i18n-ok
                };
                replaces.as_str()
            } else {
                l(x_.ru, x_.en, x_.pt)
            };
            pick(ui, m, R::new(x, rows[c], col, m.row), x_.name, desc, x_.on, Act::Module(i));
            rows[c] += m.row + m.gap;
        }
        rows[c] += m.gap;
    }
    nav(ui, w, m, b, l("дальше", "next", "avançar"), "right");
}

fn restyle_page(ui: &mut Ui, w: &W, m: M, b: R) {
    let top = head(ui, m, b, 4, l("Что перекрасить", "What to restyle", "O que recolorir"), l("Другие программы получат твою палитру. Снимешь галочку, и yami их не тронет.", "Other programs get your palette. Untick one and yami leaves it alone.", "Outros programas recebem sua paleta. Desmarque e o yami não mexe neles."));
    let col = (b.w - 2 * m.gap * 2) / 3;
    let h = m.btn + m.gap;
    let rows = RESTYLE.len().div_ceil(3) as i32;
    let y = middle(m, b, top, rows * (h + m.gap));
    for (i, (_, name)) in RESTYLE.iter().enumerate() {
        let (cx, cy) = (i as i32 % 3, i as i32 / 3);
        pick(ui, m, R::new(b.x + cx * (col + m.gap * 2), y + cy * (h + m.gap), col, h), name, "", w.restyle[i], Act::Restyle(i));
    }
    nav(ui, w, m, b, l("дальше", "next", "avançar"), "right");
}

fn confirm(ui: &mut Ui, w: &W, m: M, b: R) {
    let t = ui.t;
    let top = head(ui, m, b, 5, l("Всё готово к установке", "Ready to install", "Pronto para instalar"), l("Модули собираются из исходников: обычно это несколько минут. Ничего не сломается, и всё можно вернуть.", "The modules are built from source, usually a few minutes. Nothing breaks, and everything can be undone.", "Os módulos são compilados do código-fonte, em geral alguns minutos. Nada quebra, e tudo pode ser desfeito."));
    let picked: Vec<&str> = w.mods.iter().filter(|x| x.on).map(|x| x.name).collect();
    let line = ui.f.line(m.text);
    let chip = line + m.gap * 2;
    let rows = if picked.len() > 4 { 2 } else { 1 };
    let lines_box = chip * rows + m.gap * (rows + 1) + 24 + m.gap * 2;
    let field = if !w.no_root && !w.no_password { m.gap * 3 + 24 + m.btn + 28 } else if w.no_root { m.gap * 3 + 2 * line + 8 } else { 0 };
    let mut y = middle(m, b, top, lines_box + field);
    let card = R::new(b.x, y, b.w, lines_box);
    ui.cv.pxbox(card, &BoxStyle::new(t.surface, t.border));
    ui.cv.text_in(ui.f, m.small, R::new(card.x + m.gap * 2, card.y + m.gap, card.w, 22), l("будет установлено", "will be installed", "será instalado"), t.text_dim);
    let (mut x, mut cy) = (card.x + m.gap * 2, card.y + m.gap + 28);
    if picked.is_empty() {
        ui.cv.text_in(ui.f, m.text, R::new(x, cy, card.w - m.gap * 4, chip), l("только шелл: ничего не добавлять", "only the shell: add nothing", "só o shell: não adicionar nada"), t.text);
    }
    for name in &picked {
        let cw = ui.f.width(m.text, name) + m.gap * 3;
        if x + cw > card.right() - m.gap * 2 {
            x = card.x + m.gap * 2;
            cy += chip + m.gap;
        }
        let r = R::new(x, cy, cw, chip);
        ui.cv.pxbox(r, &BoxStyle::new(t.selection.alpha(0.45), t.ember));
        ui.cv.text_center(ui.f, m.text, r, name, t.text_hi);
        x += cw + m.gap;
    }
    y += lines_box + m.gap * 3;
    if w.no_root {
        ui.cv.text_wrap(ui.f, m.text, R::new(b.x, y, b.w, 2 * line + 8), l("sudo нет: нужные пакеты покажу одной командой в настройках → обновления", "there is no sudo: the packages you need are shown as one command in settings → updates", "não há sudo: os pacotes necessários aparecem como um comando em configurações → atualizações"), t.text_dim, 2);
    } else if !w.no_password {
        ui.cv.text_in(ui.f, m.small, R::new(b.x, y, b.w, 20), l("пароль администратора для пакетов (пусто: только домашняя папка)", "administrator password, for the packages (empty: home folder only)", "senha de administrador, para os pacotes (vazio: só a pasta pessoal)"), t.text_dim);
        y += 24;
        pass_field(ui, m, R::new(b.x, y, b.w, m.btn), &w.field, l("пароль", "password", "senha"));
        y += m.btn + 6;
        if w.wrong {
            ui.cv.text_in(ui.f, m.small, R::new(b.x, y, b.w, 20), l("неверный пароль", "wrong password", "senha incorreta"), t.ember);
        }
    }
    nav(ui, w, m, b, l("установить", "install", "instalar"), "check");
}

fn install(ui: &mut Ui, w: &W, m: M, b: R) {
    let t = ui.t;
    let title = if w.failed.is_some() { l("Не получилось", "It did not work", "Não deu certo") } else { l("Устанавливаю", "Installing", "Instalando") };
    let sub = if w.failed.is_some() { l("Ничего не сломалось: прежние программы на месте.", "Nothing is broken: the old programs are still in place.", "Nada quebrou: os programas antigos continuam no lugar.") } else { l("Обычно это несколько минут. Прежние программы работают, пока всё не соберётся.", "Usually a few minutes. The old programs keep working until everything is built.", "Em geral alguns minutos. Os programas antigos continuam funcionando até tudo ser compilado.") };
    let mut y = head(ui, m, b, 0, title, sub);
    if let Some((k, n, s)) = &w.step {
        let lh = ui.f.line(m.text);
        ui.cv.text_in(ui.f, m.text, R::new(b.x, y, b.w - 160, lh + 4), s, t.text);
        let done = if *n == 100 { format!("{k}%") } else { format!("{k}/{n}") };
        ui.text_right(m.text, R::new(b.x, y, b.w, lh + 4), &done, t.text_hi);
        y += lh + m.gap;
        ui.cv.gauge(R::new(b.x, y, b.w, m.check / 2 + 4), 48, *k as f32 / (*n).max(1) as f32, t.red, t.surface_hi, 2);
        y += m.check / 2 + 4 + m.gap * 2;
    }
    // перерисовка — по строкам установщика: без постоянной анимации сборке достаётся весь процессор
    let tail = if w.failed.is_some() { m.btn + m.gap * 2 + 2 * ui.f.line(m.text) + 8 } else { 0 };
    let log = R::new(b.x, y, b.w, b.bottom() - tail - y);
    ui.cv.pxbox(log, &BoxStyle::new(t.code_bg, t.border));
    ui.cv.push_clip(log.inset(m.gap));
    let lh = ui.f.line(m.small) + 4;
    let fits = ((log.h - m.gap * 2) / lh).max(1) as usize;
    for (i, line) in w.lines.iter().skip(w.lines.len().saturating_sub(fits)).enumerate() {
        ui.cv.text(ui.f, m.small, log.x + m.gap + 2, log.y + m.gap + i as i32 * lh, line, t.text_dim);
    }
    ui.cv.pop_clip();
    if let Some(why) = &w.failed {
        let lh = ui.f.line(m.text);
        ui.cv.text_wrap(ui.f, m.text, R::new(b.x, b.bottom() - m.btn - m.gap - 2 * lh - 8, b.w, 2 * lh + 8), why, t.ember, 2);
        let bw = m.btn * 4;
        btn(ui, m, R::new(b.x, b.bottom() - m.btn, bw * 3 / 4, m.btn), l("назад", "back", "voltar"), "left", Kind::Plain, false, Act::Back);
        btn(ui, m, R::new(b.right() - bw, b.bottom() - m.btn, bw, m.btn), l("повторить", "try again", "tentar de novo"), "right", Kind::Primary, false, Act::Retry);
    }
}

fn done(ui: &mut Ui, w: &W, m: M, b: R) {
    let t = ui.t;
    let rows = [("Mod+Space", l("запуск программ", "launcher", "lançador")), ("Mod+S", l("настройки", "settings", "configurações")), ("Mod+V", l("буфер обмена", "clipboard", "área de transferência")), ("Mod+E", l("файлы", "files", "arquivos")), ("Print", l("скриншот", "screenshot", "captura de tela")), ("Mod+Shift+/", l("все клавиши", "all keys", "todas as teclas"))];
    let chosen = |id: &str| w.mods.iter().any(|x| x.id == id && x.on);
    let list: Vec<_> = rows.into_iter().filter(|(k, _)| (*k != "Mod+E" || chosen("files")) && (*k != "Print" || chosen("shot"))).collect();
    let hl = ui.f.line(m.hero);
    let sl = ui.f.line(m.text);
    let key_h = m.btn - 4;
    let total = hl + 12 + sl + m.gap * 4 + list.len() as i32 * (key_h + m.gap);
    let mut y = middle(m, b, b.y, total);
    ui.cv.text_in(ui.f, m.hero, R::new(b.x, y, b.w, hl + 4), l("Готово", "All set", "Tudo pronto"), t.text_hi);
    y += hl + 12;
    ui.cv.text_in(ui.f, m.text, R::new(b.x, y, b.w, sl), l("Вот что стоит запомнить. Клавиши можно поменять в настройках.", "Here is what is worth remembering. Keys can be changed in the settings.", "Isto vale lembrar. As teclas podem ser alteradas nas configurações."), t.text_dim);
    y += sl + m.gap * 4;
    let kw = m.btn * 4;
    for (k, v) in list {
        let r = R::new(b.x, y, kw, key_h);
        ui.cv.pxbox(r, &BoxStyle::new(t.surface_hi, t.border).bevel(true));
        ui.cv.text_center(ui.f, m.text, r, k, t.text_hi);
        ui.cv.text_in(ui.f, m.text, R::new(r.right() + m.gap * 2, y, b.w - kw - m.gap * 2, key_h), v, t.text);
        y += key_h + m.gap;
    }
    let bw = m.btn * 4;
    btn(ui, m, R::new(b.right() - bw, b.bottom() - m.btn, bw, m.btn), l("начать работу", "start working", "começar a usar"), "check", Kind::Primary, false, Act::Finish);
}
