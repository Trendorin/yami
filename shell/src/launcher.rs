// Меню программ и буфер обмена: шапка, поиск, список, строка подсказок.
// Tab переключает режим. Частые программы — выше. В поиске ещё и действия системы
// («сон», «заблокировать», «настройки»…), открытые окна, места в настройках («vrr» → экраны)
// и калькулятор («2+2»). Префиксы: «=» считать, «>» команда в терминале, «/» файлы в домашней
// папке, «?» только настройки. В буфере: закреплённое сверху (хранится копией, переживает
// очистку и ротацию cliphist), Ctrl+P — закрепить, Del — удалить. Ctrl+A выделяет поиск,
// Ctrl+C копирует выделенное (или ответ калькулятора).

use crate::app::{App, Modal, Msg, SKind, State};
use crate::config;
use crate::font::PX;
use crate::gfx::{BoxStyle, Img, R};
use crate::icons::{exec_argv, App as DApp};
use crate::niri::Win;
use crate::ui::{Act, Cur, Field, Tone, Ui};
use calloop::channel::Sender;
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

pub const SIZE: (i32, i32) = (720, 478);
const ROW: i32 = 30;
pub const ROWS: usize = 11;
/// миниатюра картинки из буфера в строке списка
const TW: i32 = 48;
const TH: i32 = 24;
/// больше картинок за одно открытие не декодируем
const MAX_THUMBS: usize = 60;

#[derive(Clone, Debug)]
pub enum Kind {
    App(usize),
    Clip(String),
    Pin(String),
    Sys(&'static str),
    /// ответ калькулятора (текст, который копируется)
    Calc(String),
    /// открытое окно niri
    Win(u64),
    /// вкладка настроек
    Setting(&'static str),
    /// команда в терминале
    Cmd(String),
    File(PathBuf),
}

#[derive(Clone, Debug)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub sub: String,
    pub icon: String,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct Pin {
    pub id: String,
    pub file: String,
    pub mime: String,
    pub image: bool,
    pub name: String,
    pub ts: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LTab {
    #[default]
    Apps,
    Clip,
    Share,
}

#[derive(Default)]
pub struct LState {
    pub field: Field,
    pub port_field: Field,
    pub cur: usize,
    pub scroll: usize,
    pub tab: LTab,
    pub clip_lines: Vec<String>,
    pub pins: Vec<Pin>,
    pub confirm_wipe: Option<Instant>,
    pub items: Vec<Item>,
    pub counts: HashMap<String, u32>,
    /// найденные файлы для «/»: запрос, на который они найдены, и сами пути
    pub files: (String, Vec<PathBuf>),
    /// номер поиска файлов: старые нити видят, что их запрос устарел, и бросают обход
    pub fgen: Arc<AtomicU64>,
    /// миниатюры картинок буфера (ключ — thumb_key; None — не картинка или не декодируется).
    /// Только в памяти и только пока лаунчер открыт: на диске картинка пережила бы `cliphist wipe`
    pub thumbs: HashMap<String, Option<Img>>,
    /// номер нити миниатюр: закрыли лаунчер — старая нить бросает работу
    pub tgen: Arc<AtomicU64>,
    pub clip_qr: Option<Result<crate::share::QrCode, &'static str>>,
}

/// откуда взять картинку для миниатюры
enum Src {
    Line(String),
    File(PathBuf),
}

/// у картинки из cliphist в списке вместо текста «[[ binary data 23 KiB png 1920x1080 ]]»
fn is_binary(name: &str) -> bool {
    name.starts_with("[[ binary")
}

/// «[[ binary data 23 KiB png 1920x1080 ]]» → «png 1920x1080 · 23 KiB»
fn binary_label(name: &str) -> Option<String> {
    let inner = name.trim().strip_prefix("[[ binary data")?.strip_suffix("]]")?.trim();
    let mut it = inner.split_whitespace();
    let size = format!("{} {}", it.next()?, it.next()?);
    let rest = it.collect::<Vec<_>>().join(" ");
    Some(if rest.is_empty() { size } else { format!("{rest} · {size}") })
}

/// ключ миниатюры строки списка: у записи cliphist — её номер, у закреплённой — «p:» и id
pub fn thumb_key(l: &LState, k: &Kind) -> Option<String> {
    match k {
        Kind::Clip(line) if is_binary(line.split_once('\t').map(|x| x.1).unwrap_or(line)) => Some(line.split('\t').next().unwrap_or("").to_string()),
        Kind::Pin(id) if l.pins.iter().any(|p| p.id == *id && p.image) => Some(format!("p:{id}")),
        _ => None,
    }
}

const SYS: &[(&str, &str, &str)] = &[
    ("lock", "заблокировать", "lock"),
    ("sleep", "сон", "moon"),
    ("logout", "выйти из сеанса", "exit"),
    ("reboot", "перезагрузка", "reboot"),
    ("poweroff", "выключить", "power"),
    ("settings", "настройки шелла", "gear"),
    ("clipboard", "буфер обмена", "copy"),
    ("stacking", "режим стол / лента", "float"),
    ("edit", "расставить виджеты", "grid"),
];

fn clip_dir() -> std::path::PathBuf {
    config::data_dir().join("yami-clip")
}

/// папка закреплённого: только своя (0700) — там бывают пароли и ключи; старая папка с правами
/// по умолчанию закрывается при первой записи
fn clip_dir_private() -> std::path::PathBuf {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let d = clip_dir();
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&d);
    let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
    d
}

fn score(hay: &str, q: &str) -> i32 {
    let hay = hay.to_lowercase();
    if q.is_empty() {
        return 1;
    }
    if hay.starts_with(q) {
        return 100;
    }
    if let Some(i) = hay.find(q) {
        let prev = hay[..i].chars().last();
        if matches!(prev, Some(' ') | Some('-') | Some('_') | Some('.')) {
            return 80;
        }
        return 60;
    }
    let qc: Vec<char> = q.chars().collect();
    let mut j = 0;
    for ch in hay.chars() {
        if j < qc.len() && ch == qc[j] {
            j += 1;
        }
    }
    if j == qc.len() {
        20
    } else {
        0
    }
}

impl LState {
    pub fn open(&mut self, tab: LTab, apps: &[DApp], wins: &[Win]) {
        self.field.clear();
        self.cur = 0;
        self.scroll = 0;
        self.tab = tab;
        self.confirm_wipe = None;
        self.counts = std::fs::read_to_string(config::state_dir().join("launch.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        self.load_pins();
        self.files = (String::new(), vec![]);
        self.clip_qr = None;
        self.refresh(apps, wins);
    }
    pub fn set_tab(&mut self, tab: LTab, apps: &[DApp], wins: &[Win]) {
        self.tab = tab;
        self.cur = 0;
        self.scroll = 0;
        self.clip_qr = None;
        self.refresh(apps, wins);
    }
    /// выбранная строка всегда в видимой части списка
    pub fn keep_visible(&mut self) {
        if self.cur < self.scroll {
            self.scroll = self.cur;
        }
        if self.cur >= self.scroll + ROWS {
            self.scroll = self.cur + 1 - ROWS;
        }
    }
    /// миниатюры картинок, которых ещё нет: одна нить по очереди, сначала закреплённые
    pub fn thumbs_start(&mut self, tx: Sender<Msg>) {
        let pins = self.pins.iter().filter(|p| p.image).map(|p| (format!("p:{}", p.id), Src::File(clip_dir().join(&p.file))));
        let lines = self.clip_lines.iter().filter_map(|l| thumb_key(self, &Kind::Clip(l.clone())).map(|k| (k, Src::Line(l.clone()))));
        let jobs: Vec<(String, Src)> = pins.chain(lines).filter(|(k, _)| !self.thumbs.contains_key(k)).take(MAX_THUMBS).collect();
        if jobs.is_empty() {
            return;
        }
        let my = self.tgen.fetch_add(1, Ordering::Relaxed) + 1;
        let gen = self.tgen.clone();
        let _ = std::thread::Builder::new().name("clip-thumbs".into()).spawn(move || {
            for (k, src) in jobs {
                if gen.load(Ordering::Relaxed) != my {
                    break;
                }
                let data = match &src {
                    Src::Line(l) => cliphist(&["decode"], l),
                    Src::File(p) => yami_px::paths::read_capped(p, crate::icons::MAX_FILE).unwrap_or_default(),
                };
                let img = mime_of(&data).0.starts_with("image/").then(|| crate::icons::decode(&data, crate::icons::MAX_DECODE)).flatten().map(|d| {
                    // «как обложка»: середина картинки в пропорциях клетки
                    let (x, y, cw, ch) = crate::wall::cover_rect(d.width(), d.height(), TW as u32, TH as u32);
                    crate::icons::from_rgba(&d.crop_imm(x, y, cw, ch).thumbnail_exact(TW as u32, TH as u32).to_rgba8())
                });
                drop(data);
                if tx.send(Msg::ClipThumb(k, img)).is_err() {
                    break;
                }
            }
            yami_px::trim_heap();
        });
    }
    /// лаунчер закрыт: миниатюры — из памяти, нить — бросить
    pub fn thumbs_free(&mut self) {
        self.tgen.fetch_add(1, Ordering::Relaxed);
        self.thumbs = HashMap::new();
    }

    pub fn load_pins(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let d = clip_dir();
        // папка от старых версий (права по умолчанию) закрывается и без новой записи
        if std::fs::metadata(&d).is_ok_and(|m| m.permissions().mode() & 0o077 != 0) {
            let _ = std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700));
        }
        self.pins = std::fs::read_to_string(d.join("index.json")).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    }

    pub fn refresh(&mut self, apps: &[DApp], wins: &[Win]) {
        if self.tab == LTab::Share {
            self.items.clear();
            self.cur = 0;
            return;
        }
        let q = self.field.text.trim().to_lowercase();
        let mut out: Vec<Item> = vec![];
        if self.tab == LTab::Apps {
            if let Some(items) = self.prefixed(&q) {
                self.items = items;
                self.cur = self.cur.min(self.items.len().saturating_sub(1));
                return;
            }
        }
        if self.tab == LTab::Clip {
            for p in &self.pins {
                if q.is_empty() || p.name.to_lowercase().contains(&q) {
                    out.push(Item { kind: Kind::Pin(p.id.clone()), name: p.name.clone(), sub: if p.image { "картинка".into() } else { String::new() }, icon: "pin".into() });
                }
            }
            let same = |n: &str| self.pins.iter().any(|p| p.name == n || (n.chars().count() > 40 && p.name.starts_with(&n.chars().take(40).collect::<String>())));
            for l in &self.clip_lines {
                let name = l.split_once('\t').map(|x| x.1).unwrap_or(l).split_whitespace().collect::<Vec<_>>().join(" ");
                if (!q.is_empty() && !name.to_lowercase().contains(&q)) || same(&name) {
                    continue;
                }
                let img = is_binary(&name);
                out.push(Item { kind: Kind::Clip(l.clone()), name, sub: if img { "картинка".into() } else { String::new() }, icon: String::new() });
                if out.len() > 120 {
                    break;
                }
            }
        } else {
            let mut scored: Vec<(f32, usize)> = vec![];
            for (i, a) in apps.iter().enumerate() {
                let s = (score(&a.name, &q) as f32)
                    .max(score(&a.generic, &q) as f32 * 0.8)
                    .max(score(&a.keywords, &q) as f32 * 0.6)
                    .max(score(&a.comment, &q) as f32 * 0.3)
                    .max(score(&a.id, &q) as f32 * 0.5);
                if s > 0.0 {
                    let used = *self.counts.get(&a.id).unwrap_or(&0) as f32;
                    scored.push((s + used.min(40.0) * if q.is_empty() { 1.0 } else { 0.25 }, i));
                }
            }
            scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal).then(apps[a.1].name.cmp(&apps[b.1].name)));
            if !q.is_empty() {
                if crate::calc::looks_like(&q) {
                    out.extend(calc_item(&q));
                }
                for (id, name, icon) in SYS {
                    if score(name, &q) >= 60 || score(crate::i18n::tr(name), &q) >= 60 {
                        out.push(Item { kind: Kind::Sys(id), name: name.to_string(), sub: "система".into(), icon: icon.to_string() });
                    }
                }
            }
            for (_, i) in scored.into_iter().take(80) {
                let a = &apps[i];
                out.push(Item { kind: Kind::App(i), name: a.name.clone(), sub: if a.generic.is_empty() { a.comment.clone() } else { a.generic.clone() }, icon: a.icon.clone() });
            }
            if !q.is_empty() {
                // открытые окна — по заголовку и программе; после программ: запуск чаще
                for w in wins.iter().filter(|w| score(&w.title, &q) >= 60 || score(&w.app_id, &q) >= 60).take(8) {
                    out.push(Item { kind: Kind::Win(w.id), name: w.title.clone(), sub: crate::tf!("окно · {}", "window · {}", "janela · {}", w.app_id), icon: w.app_id.clone() });
                }
                out.extend(settings_items(&q, 60).into_iter().take(3));
            }
        }
        self.items = out;
        self.cur = self.cur.min(self.items.len().saturating_sub(1));
    }
}

impl LState {
    /// строки для запроса с префиксом (= > / ?); None — префикса нет, обычный поиск
    fn prefixed(&self, q: &str) -> Option<Vec<Item>> {
        let mut c = q.chars();
        let p = c.next()?;
        let rest = c.as_str().trim();
        Some(match p {
            '=' => calc_item(q).into_iter().collect(),
            '>' if !rest.is_empty() => {
                // команду показываем как набрана (без нижнего регистра)
                let cmd = self.field.text.trim().trim_start_matches('>').trim().to_string();
                vec![Item { kind: Kind::Cmd(cmd.clone()), name: cmd, sub: "выполнить в терминале".into(), icon: "term".into() }]
            }
            '>' => vec![],
            '?' => settings_items(rest, 20),
            '/' => {
                if self.files.0 != rest {
                    return Some(vec![]);
                }
                self.files.1.iter().map(|f| {
                    let shown = yami_px::paths::pretty(f);
                    let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    Item { kind: Kind::File(f.clone()), name, sub: shown, icon: if f.is_dir() { "folder".into() } else { "copy".into() } }
                }).collect()
            }
            _ => return None,
        })
    }
}

fn calc_item(q: &str) -> Option<Item> {
    let v = crate::calc::eval(q)?;
    let s = crate::calc::format(v);
    Some(Item { kind: Kind::Calc(s.clone()), name: format!("= {s}"), sub: "Enter — скопировать".into(), icon: "calc".into() })
}

/// вкладки настроек, подходящие к запросу: по названию (на обоих языках) и словам вкладки
fn settings_items(q: &str, min: i32) -> Vec<Item> {
    let mut hits: Vec<(i32, Item)> = vec![];
    for tb in crate::settings::tabs() {
        let (id, name, icon, words) = (&tb.id, tb.name, tb.icon, tb.find);
        let s = if q.is_empty() {
            1
        } else {
            score(name, q).max(score(crate::i18n::tr(name), q)).max(words.split(' ').map(|w| score(w, q)).max().unwrap_or(0))
        };
        if s >= min {
            hits.push((s, Item { kind: Kind::Setting(id), name: crate::tf!("настройки → {}", "settings → {}", "configurações → {}", crate::i18n::tr(name)), sub: String::new(), icon: icon.to_string() }));
        }
    }
    hits.sort_by_key(|h| std::cmp::Reverse(h.0));
    hits.into_iter().map(|h| h.1).collect()
}

/// поиск файлов по имени в домашней папке: обход без скрытых папок, не глубже 8 уровней и не
/// больше 60 тысяч записей; результат — Msg::LFiles. Новый запрос гасит старый обход.
pub fn find_files(tx: Sender<Msg>, gen: Arc<AtomicU64>, q: String) {
    let my = gen.fetch_add(1, Ordering::Relaxed) + 1;
    if q.is_empty() {
        let _ = tx.send(Msg::LFiles(q, vec![]));
        return;
    }
    std::thread::spawn(move || {
        // чуть подождать: пока набирают слово, обходить диск незачем
        std::thread::sleep(std::time::Duration::from_millis(120));
        let ql = q.to_lowercase();
        let mut found: Vec<(i32, PathBuf)> = vec![];
        let mut stack = vec![(config::home(), 0u32)];
        let mut seen = 0u32;
        while let Some((dir, depth)) = stack.pop() {
            if gen.load(Ordering::Relaxed) != my {
                return;
            }
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                seen += 1;
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let s = score(&name, &ql);
                if s >= 60 {
                    found.push((s, e.path()));
                }
                // ссылки на папки не обходим: петли и чужие диски
                if depth < 8 && e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    stack.push((e.path(), depth + 1));
                }
            }
            if seen > 60_000 || found.len() > 400 {
                break;
            }
        }
        found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.as_os_str().len().cmp(&b.1.as_os_str().len())));
        if gen.load(Ordering::Relaxed) == my {
            let _ = tx.send(Msg::LFiles(q, found.into_iter().take(60).map(|f| f.1).collect()));
        }
    });
}

/// команда в терминале (Terminal=true в .desktop и «>» в поиске)
pub fn in_term(argv: Vec<String>) -> Vec<String> {
    let t = term();
    let mut pre = vec![t.clone()];
    match t.as_str() {
        "kitty" => {}
        "wezterm" => pre.extend(["start".into(), "--".into()]),
        _ => pre.push("-e".into()),
    }
    pre.extend(argv);
    pre
}

pub fn load_clip(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let o = std::process::Command::new("cliphist").arg("list").output();
        let lines = o.map(|o| String::from_utf8_lossy(&o.stdout).lines().filter(|l| !l.is_empty()).map(String::from).collect()).unwrap_or_default();
        let _ = tx.send(Msg::Clip(lines));
    });
}

fn mime_of(d: &[u8]) -> (&'static str, &'static str) {
    if d.starts_with(b"\x89PNG") {
        ("image/png", "png")
    } else if d.starts_with(&[0xff, 0xd8, 0xff]) {
        ("image/jpeg", "jpg")
    } else if d.starts_with(b"GIF8") {
        ("image/gif", "gif")
    } else if d.len() > 12 && &d[..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        ("image/webp", "webp")
    } else {
        ("text/plain;charset=utf-8", "txt")
    }
}

fn cliphist(args: &[&str], input: &str) -> Vec<u8> {
    use std::io::Write;
    let Ok(mut c) = std::process::Command::new("cliphist").args(args).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn() else { return vec![] };
    if let Some(mut i) = c.stdin.take() {
        let _ = i.write_all(input.as_bytes());
    }
    c.wait_with_output().map(|o| o.stdout).unwrap_or_default()
}

fn save_pins(p: &[Pin]) {
    let _ = config::write_atomic(&clip_dir_private().join("index.json"), &serde_json::to_string_pretty(p).unwrap_or_default());
}

fn wl_copy(data: Vec<u8>, mime: &str) {
    use std::io::Write;
    if let Ok(mut c) = std::process::Command::new("setsid").args(["-f", "wl-copy", "--type", mime]).stdin(std::process::Stdio::piped()).spawn() {
        if let Some(mut i) = c.stdin.take() {
            let _ = i.write_all(&data);
        }
        let _ = c.wait();
    }
}

impl App {
    fn lrefresh(&mut self) {
        self.st.launcher.refresh(&self.st.apps, &self.st.niri.wins);
        self.dirty(&SKind::Modal);
    }

    pub fn launcher_run(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::App(ai) => {
                if let Some(a) = self.st.apps.get(ai).cloned() {
                    *self.st.launcher.counts.entry(a.id.clone()).or_insert(0) += 1;
                    let _ = config::write_atomic(&config::state_dir().join("launch.json"), &serde_json::to_string(&self.st.launcher.counts).unwrap_or_default());
                    let mut argv = exec_argv(&a.exec);
                    if a.terminal {
                        argv = in_term(argv);
                    }
                    App::spawn(&argv);
                }
            }
            Kind::Clip(line) => {
                std::thread::spawn(move || {
                    let data = cliphist(&["decode"], &line);
                    let (mime, _) = mime_of(&data);
                    wl_copy(data, mime);
                });
            }
            Kind::Pin(id) => {
                if let Some(p) = self.st.launcher.pins.iter().find(|p| p.id == id) {
                    let path = clip_dir().join(&p.file);
                    let mime = p.mime.clone();
                    std::thread::spawn(move || {
                        if let Ok(d) = std::fs::read(path) {
                            wl_copy(d, &mime);
                        }
                    });
                }
            }
            Kind::Sys(a) => {
                self.close_modal();
                self.sys_action(a);
                return;
            }
            Kind::Calc(s) => {
                std::thread::spawn(move || wl_copy(s.into_bytes(), "text/plain;charset=utf-8"));
            }
            Kind::Win(id) => self.st.niri.act(serde_json::json!({"FocusWindow": {"id": id}})),
            Kind::Setting(tab) => {
                self.close_modal();
                self.toggle_modal(Modal::Settings, false);
                self.settings_click(&Act::STab(tab));
                self.dirty(&SKind::Modal);
                return;
            }
            Kind::Cmd(cmd) => {
                // окно не закрывается само: ответ команды виден, Enter закрывает
                let line = format!("{cmd}; printf '\\n[Enter]'; read _");
                App::spawn(&in_term(vec!["sh".into(), "-c".into(), line]));
            }
            Kind::File(p) => {
                App::spawn(&["xdg-open".into(), p.to_string_lossy().to_string()]);
            }
        }
        self.close_modal();
    }

    pub fn sys_action(&mut self, a: &str) {
        match a {
            "lock" => self.lock_now(),
            "sleep" => {
                self.lock_now();
                App::sh("sleep 0.6; systemctl suspend");
            }
            "logout" => self.st.niri.act(serde_json::json!({"Quit": {"skip_confirmation": true}})),
            "reboot" => App::spawn(&["systemctl".into(), "reboot".into()]),
            "poweroff" => App::spawn(&["systemctl".into(), "poweroff".into()]),
            "settings" => self.toggle_modal(Modal::Settings, false),
            "clipboard" => self.toggle_modal(Modal::Launcher, true),
            "stacking" => self.ipc("stacking"),
            "edit" => self.set_editing(true),
            _ => {}
        }
    }

    pub fn launcher_pin(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::Pin(id) => {
                let mut pins = self.st.launcher.pins.clone();
                if let Some(p) = pins.iter().find(|p| p.id == id) {
                    let _ = std::fs::remove_file(clip_dir().join(&p.file));
                }
                pins.retain(|p| p.id != id);
                save_pins(&pins);
            }
            Kind::Clip(line) => {
                let data = cliphist(&["decode"], &line);
                if data.is_empty() {
                    return;
                }
                let id = format!("{:016x}", yami_px::fnv(yami_px::FNV0, &data));
                let mut pins = self.st.launcher.pins.clone();
                if pins.iter().any(|p| p.id == id) {
                    return;
                }
                let (mime, ext) = mime_of(&data);
                let file = format!("{id}.{ext}");
                let _ = std::fs::write(clip_dir_private().join(&file), &data);
                let name = if ext == "txt" {
                    String::from_utf8_lossy(&data).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(300).collect()
                } else {
                    line.split_once('\t').map(|x| x.1.trim().to_string()).unwrap_or_default()
                };
                let ts = unsafe { libc::time(std::ptr::null_mut()) };
                pins.insert(0, Pin { id, file, mime: mime.into(), image: ext != "txt", name, ts });
                save_pins(&pins);
            }
            _ => return,
        }
        self.st.launcher.load_pins();
        self.st.launcher.thumbs_start(self.tx.clone());
        self.lrefresh();
    }

    pub fn launcher_del(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::Pin(_) => self.launcher_pin(i),
            Kind::Clip(line) => {
                cliphist(&["delete"], &line);
                self.st.launcher.clip_lines.retain(|l| *l != line);
                self.lrefresh();
            }
            _ => {}
        }
    }

    pub fn launcher_wipe(&mut self) {
        match self.st.launcher.confirm_wipe {
            Some(t) if t.elapsed().as_secs() < 3 => {
                self.st.launcher.confirm_wipe = None;
                cliphist(&["wipe"], "");
                self.st.launcher.clip_lines.clear();
                self.lrefresh();
            }
            _ => {
                self.st.launcher.confirm_wipe = Some(Instant::now());
                self.dirty(&SKind::Modal);
            }
        }
    }

    pub fn launcher_qr(&mut self, i: usize) {
        let Some(it) = self.st.launcher.items.get(i).cloned() else { return };
        match it.kind {
            Kind::Pin(id) => {
                let Some(p) = self.st.launcher.pins.iter().find(|p| p.id == id).cloned() else { return };
                let path = clip_dir().join(&p.file);
                if p.image {
                    if path.is_file() {
                        self.st.share.add_files(vec![path]);
                        if !self.st.share.active {
                            self.st.share.start(self.st.set.share_port, &self.st.t, self.tx.clone());
                        }
                        self.open_launcher(LTab::Share);
                    }
                } else if let Ok(data) = std::fs::read(&path) {
                    self.st.launcher.clip_qr = Some(clip_text_qr(&data));
                    self.dirty(&SKind::Modal);
                }
            }
            Kind::Clip(line) => {
                let data = cliphist(&["decode"], &line);
                if data.is_empty() {
                    return;
                }
                let (_, ext) = mime_of(&data);
                if ext != "txt" {
                    let dir = std::env::temp_dir().join("yami-share-clip");
                    if attach_clip_bytes_to_share(&mut self.st.share, &dir, &data, ext) {
                        if !self.st.share.active {
                            self.st.share.start(self.st.set.share_port, &self.st.t, self.tx.clone());
                        }
                        self.open_launcher(LTab::Share);
                    }
                } else {
                    self.st.launcher.clip_qr = Some(clip_text_qr(&data));
                    self.dirty(&SKind::Modal);
                }
            }
            _ => {}
        }
    }

    fn parse_share_port(&mut self) -> Option<u16> {
        match self.st.launcher.port_field.text.trim().parse::<u16>() {
            Ok(p) if p >= 1024 => Some(p),
            _ => {
                self.st.share.error = crate::i18n::tr("порт: 1024–65535").to_string();
                self.dirty(&SKind::Modal);
                None
            }
        }
    }

    fn apply_share_port(&mut self, port: u16) {
        self.st.set.share_port = port;
        self.save_now();
        self.st.share.fw_error.clear();
        self.st.share.fw_note.clear();
        if crate::proc::have("ufw") && crate::proc::have("pkexec") {
            self.close_modal();
        }
        self.st.share.fw_busy = true;
        crate::share::apply_ufw_rule(port, self.tx.clone());
    }

    pub fn share_toggle(&mut self) {
        if self.st.share.active {
            self.st.share.stop();
        } else {
            let Some(port) = self.parse_share_port() else { return };
            if port != self.st.set.share_port {
                self.apply_share_port(port);
            }
            self.st.share.start(port, &self.st.t, self.tx.clone());
        }
        if self.st.modal.is_some() {
            self.dirty(&SKind::Modal);
        }
    }

    pub fn share_apply_port(&mut self) {
        let Some(port) = self.parse_share_port() else { return };
        self.st.share.error.clear();
        self.apply_share_port(port);
        if self.st.modal.is_some() {
            self.dirty(&SKind::Modal);
        }
    }

    pub fn share_copy(&self) {
        if !self.st.share.url.is_empty() {
            let u = self.st.share.url.clone();
            std::thread::spawn(move || wl_copy(u.into_bytes(), "text/plain;charset=utf-8"));
        }
    }

    pub fn share_pick(&mut self) {
        self.close_modal();
        let tx = self.tx.clone();
        let from = yami_px::paths::home();
        let title = crate::i18n::tr("выбрать файлы").to_string();
        std::thread::spawn(move || {
            let paths = crate::proc::files_pick_many(&from, &title);
            let _ = tx.send(Msg::SharePicked(paths));
        });
    }

    pub fn launcher_key(&mut self, e: &KeyEvent) {
        if self.st.launcher.tab == LTab::Share && self.st.field_focus == Some("share_port") {
            match e.keysym {
                Keysym::Escape => self.st.field_focus = None,
                Keysym::Return | Keysym::KP_Enter => {
                    self.st.field_focus = None;
                    self.share_apply_port();
                    return;
                }
                _ => {
                    let mods = self.mods;
                    if App::edit_field(&mut self.st.launcher.port_field, e, &mods) {
                        self.st.launcher.port_field.text.retain(|c| c.is_ascii_digit());
                        self.st.launcher.port_field.text.truncate(5);
                        let len = self.st.launcher.port_field.text.len();
                        self.st.launcher.port_field.cur = self.st.launcher.port_field.cur.min(len);
                    }
                }
            }
            self.dirty(&SKind::Modal);
            return;
        }
        let l = &mut self.st.launcher;
        if l.tab == LTab::Clip && l.clip_qr.is_some() && e.keysym == Keysym::Escape {
            l.clip_qr = None;
            self.dirty(&SKind::Modal);
            return;
        }
        let n = l.items.len();
        match e.keysym {
            Keysym::Escape => self.close_modal(),
            Keysym::Return | Keysym::KP_Enter if l.tab == LTab::Share => {
                self.share_toggle();
                return;
            }
            Keysym::Return | Keysym::KP_Enter => {
                let c = l.cur;
                self.launcher_run(c);
            }
            Keysym::Down => l.cur = (l.cur + 1).min(n.saturating_sub(1)),
            Keysym::Up => l.cur = l.cur.saturating_sub(1),
            Keysym::Page_Down => l.cur = (l.cur + 10).min(n.saturating_sub(1)),
            Keysym::Page_Up => l.cur = l.cur.saturating_sub(10),
            Keysym::Tab | Keysym::ISO_Left_Tab => {
                let back = e.keysym == Keysym::ISO_Left_Tab || self.mods.shift;
                let next = match (l.tab, back) {
                    (LTab::Apps, false) | (LTab::Share, true) => LTab::Clip,
                    (LTab::Clip, false) | (LTab::Apps, true) => LTab::Share,
                    (LTab::Share, false) | (LTab::Clip, true) => LTab::Apps,
                };
                l.set_tab(next, &self.st.apps, &self.st.niri.wins);
                if next == LTab::Clip {
                    load_clip(self.tx.clone());
                }
            }
            Keysym::Delete if l.tab == LTab::Clip && l.field.cur == l.field.text.chars().count() => {
                let c = l.cur;
                self.launcher_del(c);
            }
            // Ctrl+P и Ctrl+C — в любой раскладке (keys::latin_letter)
            _ if self.mods.ctrl && l.tab == LTab::Clip && crate::keys::latin_letter(e) == Some('P') => {
                let c = l.cur;
                self.launcher_pin(c);
            }
            // Ctrl+C: выделенный поиск или ответ калькулятора
            _ if self.mods.ctrl && crate::keys::latin_letter(e) == Some('C') => {
                let text = l.field.selected().map(String::from).or_else(|| match l.items.get(l.cur).map(|i| &i.kind) {
                    Some(Kind::Calc(s)) => Some(s.clone()),
                    _ => None,
                });
                if let Some(t) = text {
                    std::thread::spawn(move || wl_copy(t.into_bytes(), "text/plain;charset=utf-8"));
                }
            }
            _ if l.tab != LTab::Share => {
                let mods = self.mods;
                if App::edit_field(&mut self.st.launcher.field, e, &mods) {
                    self.launcher_typed();
                }
            }
            _ => {}
        }
        self.st.launcher.keep_visible();
        self.dirty(&SKind::Modal);
    }

    /// запрос поменялся (набрали или пришёл `msg launcher ТЕКСТ`): список с начала, `/` ищет файлы
    pub fn launcher_typed(&mut self) {
        let l = &mut self.st.launcher;
        l.cur = 0;
        l.scroll = 0;
        let q = l.field.text.trim();
        if l.tab == LTab::Apps && q.starts_with('/') {
            let rest = q[1..].trim().to_lowercase();
            if rest != l.files.0 {
                find_files(self.tx.clone(), l.fgen.clone(), rest);
            }
        }
        l.refresh(&self.st.apps, &self.st.niri.wins);
    }
}

pub fn term() -> String {
    for t in ["kitty", "foot", "alacritty", "wezterm", "ghostty", "konsole", "xterm"] {
        if std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(t).exists())).unwrap_or(false) {
            return t.into();
        }
    }
    "xterm".into()
}


pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let t = ui.t;
    let l = &st.launcher;
    let fr = R::new(0, 0, w - 8, h - 8);
    ui.frame(fr, true);
    // шапка
    let head = R::new(1, 1, fr.w - 2, 30);
    let title = match l.tab {
        LTab::Apps => "программы",
        LTab::Clip => "буфер обмена",
        LTab::Share => "обмен",
    };
    let count = match l.tab {
        LTab::Share => if st.share.active { st.share.files.len().to_string() } else { String::new() },
        _ => l.items.len().to_string(),
    };
    ui.header(head, "", title, &count);
    let mut bx = head.right() - 6;
    let mut btn = |ui: &mut Ui, label: &str, icon: &str, tone: Tone, sel: bool, act: Act| {
        let bw = if label.is_empty() { 22 } else { ui.w(PX, label) + 12 + if icon.is_empty() { 0 } else { 18 } };
        bx -= bw;
        ui.button(R::new(bx, head.y + 5, bw, 20), label, icon, tone, sel, true, act);
        bx -= 6;
    };
    btn(ui, "", "close", Tone::Danger, false, Act::CloseModal);
    btn(ui, "ОБМЕН", "", Tone::Plain, l.tab == LTab::Share, Act::LTab(LTab::Share));
    btn(ui, "БУФЕР", "", Tone::Plain, l.tab == LTab::Clip, Act::LTab(LTab::Clip));
    btn(ui, "ПРОГРАММЫ", "", Tone::Plain, l.tab == LTab::Apps, Act::LTab(LTab::Apps));
    if l.tab == LTab::Clip {
        let wiping = l.confirm_wipe.map(|t| t.elapsed().as_secs() < 3).unwrap_or(false);
        btn(ui, if wiping { "точно? ещё раз" } else { "очистить" }, "trash", Tone::Danger, false, Act::LWipe);
    }
    if l.tab == LTab::Share {
        draw_share(ui, st, fr, head);
        return;
    }

    // поиск
    let fx = 14;
    let field = R::new(fx, head.bottom() + 13, fr.w - 28, 32);
    ui.field(field, &l.field, if l.tab == LTab::Clip { "ищи в буфере… Enter — скопировать" } else { "напиши… Enter — запустить" }, true, false, Act::Block);

    // список
    let list = R::new(fx, field.bottom() + 8, fr.w - 28, fr.h - field.bottom() - 8 - 34);
    let rows = (list.h / ROW).max(1) as usize;
    let scroll = l.scroll.min(l.items.len().saturating_sub(rows));
    ui.cv.push_clip(list);
    for (vi, it) in l.items.iter().enumerate().skip(scroll).take(rows + 1) {
        let r = R::new(list.x, list.y + (vi - scroll) as i32 * ROW, list.w, ROW);
        let cur = vi == l.cur;
        let hov = ui.hover(r);
        if cur || hov {
            ui.cv.pxbox(R::new(r.x, r.y, r.w, r.h - 2), &BoxStyle::new(if cur { t.selection } else { t.surface_hi }, if cur { t.red } else { t.border }).bevel(cur));
        }
        if cur {
            ui.cv.fill(R::new(r.x, r.y + 2, 2, r.h - 6), t.ember);
        }
        ui.zone(r, Act::LRun(vi), Cur::Pointer);
        let mut x = r.x + 10;
        let cy = r.y + (r.h - 2) / 2;
        match &it.kind {
            Kind::Pin(_) => {
                ui.cv.icon("pin", x, cy - 7, 2, t.red);
                x += 24;
            }
            Kind::Sys(_) | Kind::Calc(_) | Kind::Setting(_) | Kind::Cmd(_) | Kind::File(_) => {
                ui.cv.icon(&it.icon, x, cy - 7, 2, t.red);
                x += 26;
            }
            Kind::App(_) | Kind::Win(_) => {
                ui.app_icon(&it.icon, x, cy - 8, 16, 1.0);
                x += 26;
            }
            Kind::Clip(_) => {}
        }
        // картинка из буфера: миниатюра (пока не готова или не декодируется — значок)
        let thumb = (l.tab == LTab::Clip).then(|| thumb_key(l, &it.kind)).flatten();
        if let Some(k) = &thumb {
            let b = R::new(x, cy - TH / 2 - 1, TW + 2, TH + 2);
            ui.cv.fill(b, t.border);
            match l.thumbs.get(k) {
                Some(Some(img)) => ui.cv.blit(img, b.x + 1, b.y + 1),
                _ => {
                    ui.cv.fill(R::new(b.x + 1, b.y + 1, TW, TH), t.surface);
                    ui.cv.icon("image", b.x + (b.w - 14) / 2, b.y + (b.h - 14) / 2, 2, t.text_faint);
                }
            }
            x += TW + 10;
        }
        let label = thumb.as_ref().and_then(|_| binary_label(&it.name));
        let name = label.as_deref().unwrap_or(&it.name);
        let maxw = if l.tab == LTab::Clip { list.w - (x - r.x) - 96 } else { list.w - 230 };
        // имена программ, окон, файлов и команды — как есть, без перевода интерфейса
        let raw = !matches!(it.kind, Kind::Sys(_));
        let nw = if raw { ui.f.width(PX, name) } else { ui.w(PX, name) }.min(maxw);
        let nc = if cur { t.text_hi } else { t.text };
        if raw {
            ui.cv.text_in(ui.f, PX, R::new(x, r.y, nw, r.h - 2), name, nc);
        } else {
            ui.text_in(PX, R::new(x, r.y, nw, r.h - 2), name, nc);
        }
        if !it.sub.is_empty() {
            let sx = x + nw + 10;
            let sr = R::new(sx, r.y, r.right() - sx - 16, r.h - 2);
            let sc = if cur { t.text } else { t.text_faint };
            if matches!(it.kind, Kind::App(_) | Kind::File(_)) {
                ui.cv.text_in(ui.f, PX, sr, &it.sub, sc);
            } else {
                ui.text_in(PX, sr, &it.sub, sc);
            }
        }
        // in clipboard: qr/share, pin and delete on the selected row
        if l.tab == LTab::Clip && (cur || hov) {
            let pinned = matches!(it.kind, Kind::Pin(_));
            ui.button(R::new(r.right() - 80, r.y + 3, 24, 22), "", "qr", Tone::Ghost, false, true, Act::LQr(vi));
            ui.button(R::new(r.right() - 54, r.y + 3, 24, 22), "", "pin", Tone::Ghost, pinned, true, Act::LPin(vi));
            ui.button(R::new(r.right() - 28, r.y + 3, 24, 22), "", "trash", Tone::Ghost, false, true, Act::LDel(vi));
        }
    }
    ui.cv.pop_clip();
    if l.items.is_empty() {
        let q = l.field.text.trim();
        let msg = if l.tab == LTab::Clip {
            "буфер пуст"
        } else if q.starts_with('/') && q.len() > 1 && l.files.0 != q[1..].trim().to_lowercase() {
            "ищу файлы…"
        } else if q == "/" {
            "/имя — файлы в домашней папке"
        } else if q == ">" {
            ">команда — выполнить в терминале"
        } else {
            "ничего не нашлось"
        };
        ui.text_center(PX, R::new(list.x, list.y + 20, list.w, 20), msg, t.text_faint);
    }

    // строка подсказок внизу
    let hint = if l.tab == LTab::Clip { "Enter — скопировать · Ctrl+P — закрепить · Del — удалить · Tab — обмен" } else { "Enter — запустить · = считать · > команда · / файлы · ? настройки · Tab — буфер" };
    ui.text_in(PX, R::new(fx, fr.h - 30, fr.w - 28, 20), hint, t.text_faint);

    if l.tab == LTab::Clip {
        if let Some(res) = &l.clip_qr {
            ui.zone(fr, Act::LQrClose, Cur::Default);
            let _card = match res {
                Ok(qr) => {
                    let cell = (220 / (qr.size as i32 + 4)).max(3);
                    let qpx = (qr.size as i32 + 4) * cell;
                    let cw = qpx + 28;
                    let ch = qpx + 28;
                    let card = R::new(fr.x + (fr.w - cw) / 2, fr.y + (fr.h - ch) / 2, cw, ch);
                    ui.cv.pxbox(card, &BoxStyle::new(t.panel, t.border));
                    ui.zone(card, Act::Block, Cur::Default);
                    draw_qr_box(ui, qr, card.x + (card.w - qpx) / 2, card.y + 14, cell);
                    card
                }
                Err(msg) => {
                    let cw = (ui.w(PX, msg) + 32).max(240);
                    let card = R::new(fr.x + (fr.w - cw) / 2, fr.y + (fr.h - 68) / 2, cw, 68);
                    ui.cv.pxbox(card, &BoxStyle::new(t.panel, t.border));
                    ui.zone(card, Act::Block, Cur::Default);
                    ui.text_center(PX, R::new(card.x + 12, card.y + 24, card.w - 24, 20), msg, t.red);
                    card
                }
            };
        }
    }
}

fn draw_qr_box(ui: &mut Ui, qr: &crate::share::QrCode, qx: i32, qy: i32, cell: i32) -> i32 {
    let qpx = (qr.size as i32 + 4) * cell;
    ui.cv.fill(R::new(qx, qy, qpx, qpx), yami_px::gfx::C(0xffffffff));
    for y in 0..qr.size {
        for x in 0..qr.size {
            if qr.modules[y * qr.size + x] {
                ui.cv.fill(R::new(qx + (x as i32 + 2) * cell, qy + (y as i32 + 2) * cell, cell, cell), yami_px::gfx::C(0xff000000));
            }
        }
    }
    qpx
}

fn draw_share(ui: &mut Ui, st: &State, fr: R, head: R) {
    let t = ui.t;
    let sh = &st.share;
    let fx = 14;
    let body = R::new(fx, head.bottom() + 12, fr.w - 28, fr.h - head.bottom() - 44);

    if !sh.active {
        let card = R::new(body.x, body.y + 14, body.w, 274);
        ui.cv.pxbox(card, &BoxStyle::new(t.surface, t.border));
        ui.cv.icon("qr", card.x + (card.w - 28) / 2, card.y + 20, 4, t.red);
        ui.text_center(PX, R::new(card.x + 12, card.y + 62, card.w - 24, 22), "обмен выключен", t.text_hi);
        ui.text_center(PX, R::new(card.x + 12, card.y + 86, card.w - 24, 20), "сканируй телефоном в одной Wi-Fi сети", t.text_dim);

        let port_lbl_w = ui.w(PX, "порт") + 10;
        let apply_w = ui.w(PX, if sh.fw_busy { "…" } else { "применить" }) + 20;
        let field_w = 88;
        let row_w = port_lbl_w + field_w + apply_w + 16;
        let px0 = card.x + (card.w - row_w) / 2;
        let py0 = card.y + 118;
        ui.text_in(PX, R::new(px0, py0 + 4, port_lbl_w, 20), "порт", t.text_dim);
        ui.field(R::new(px0 + port_lbl_w, py0, field_w, 28), &st.launcher.port_field, "45837", st.field_focus == Some("share_port"), false, Act::SharePortFocus);
        ui.button(R::new(px0 + port_lbl_w + field_w + 8, py0 + 1, apply_w, 26), if sh.fw_busy { "…" } else { "применить" }, "", Tone::Plain, false, !sh.fw_busy, Act::SharePortApply);

        let (msg, col) = if !sh.error.is_empty() {
            (sh.error.as_str(), t.red)
        } else if !sh.fw_error.is_empty() {
            (sh.fw_error.as_str(), t.red)
        } else if !sh.fw_note.is_empty() {
            (sh.fw_note.as_str(), t.ok)
        } else {
            ("", t.text_dim)
        };
        if !msg.is_empty() {
            let mw = ui.f.width(PX, msg);
            ui.cv.text_in(ui.f, PX, R::new(card.x + ((card.w - mw) / 2).max(12), card.y + 156, card.w - 24, 20), msg, col);
        } else if !sh.files.is_empty() {
            let msg = crate::tf!("выбрано файлов: {}", "selected files: {}", "arquivos selecionados: {}", sh.files.len());
            let mw = ui.f.width(PX, &msg);
            ui.cv.text_in(ui.f, PX, R::new(card.x + (card.w - mw) / 2, card.y + 156, card.w - 40, 20), &msg, t.text_dim);
        }
        let bw1 = ui.w(PX, "включить обмен") + 28;
        let bw2 = ui.w(PX, "выбрать файлы") + 28;
        let bx = card.x + (card.w - bw1 - bw2 - 12) / 2;
        ui.button(R::new(bx, card.y + 190, bw1, 30), "включить обмен", "wifi", Tone::Plain, true, true, Act::ShareToggle);
        ui.button(R::new(bx + bw1 + 12, card.y + 190, bw2, 30), "выбрать файлы", "plus", Tone::Plain, false, true, Act::SharePick);
        ui.text_in(PX, R::new(fx, fr.h - 30, fr.w - 28, 20), "Enter / клик — включить · Tab — программы", t.text_faint);
        return;
    }

    // Active status bar
    let bar = R::new(body.x, body.y, body.w, 30);
    ui.cv.pxbox(bar, &BoxStyle::new(t.surface, t.border));
    ui.cv.fill(R::new(bar.x + 10, bar.y + 11, 8, 8), t.ok);
    let stop_w = ui.w(PX, "остановить") + 26;
    let copy_w = 24;
    let url_r = R::new(bar.x + 26, bar.y + 4, bar.w - stop_w - copy_w - 44, 22);
    ui.cv.text_in(ui.f, PX, url_r, &sh.url, t.text_hi);
    ui.button(R::new(bar.right() - stop_w - copy_w - 12, bar.y + 4, copy_w, 22), "", "copy", Tone::Ghost, false, true, Act::ShareCopy);
    ui.button(R::new(bar.right() - stop_w - 6, bar.y + 4, stop_w, 22), "остановить", "stop", Tone::Danger, false, true, Act::ShareToggle);

    // Left column: QR code
    let qr_box = R::new(body.x, bar.bottom() + 10, 196, body.h - 40);
    ui.cv.pxbox(qr_box, &BoxStyle::new(t.surface, t.border));
    if let Some(qr) = &sh.qr {
        let cell = ((qr_box.w - 20) / (qr.size as i32 + 4)).max(3);
        let qpx = (qr.size as i32 + 4) * cell;
        let qx = qr_box.x + (qr_box.w - qpx) / 2;
        let qy = qr_box.y + 12;
        draw_qr_box(ui, qr, qx, qy, cell);
        ui.wrap(PX, R::new(qr_box.x + 8, qy + qpx + 8, qr_box.w - 16, 40), "сканируй телефоном в одной Wi-Fi сети", t.text_dim, 2);
    }

    // Right column: PC -> mobile files + Transfers
    let rx = qr_box.right() + 10;
    let rw = body.right() - rx;
    let files_box = R::new(rx, bar.bottom() + 10, rw, 174);
    ui.cv.pxbox(files_box, &BoxStyle::new(t.surface, t.border));
    let add_w = ui.w(PX, "добавить файлы") + 24;
    ui.text_in(PX, R::new(files_box.x + 10, files_box.y + 6, files_box.w - add_w - 48, 20), "файлы для отправки", t.text_hi);
    ui.button(R::new(files_box.right() - add_w - 34, files_box.y + 5, add_w, 22), "добавить файлы", "plus", Tone::Plain, false, true, Act::SharePick);
    ui.button(R::new(files_box.right() - 28, files_box.y + 5, 22, 22), "", "trash", Tone::Ghost, false, !sh.files.is_empty(), Act::ShareClear);

    let list_r = R::new(files_box.x + 6, files_box.y + 32, files_box.w - 12, files_box.h - 38);
    ui.zone(list_r, Act::ShareScroll, Cur::Default);
    ui.cv.push_clip(list_r);
    if sh.files.is_empty() {
        ui.text_center(PX, R::new(list_r.x, list_r.y + 44, list_r.w, 20), "выбрать файлы", t.text_faint);
    } else {
        let scroll = sh.scroll.min(sh.files.len().saturating_sub(5));
        for (i, f) in sh.files.iter().enumerate().skip(scroll).take(5) {
            let row = R::new(list_r.x, list_r.y + (i - scroll) as i32 * 26, list_r.w, 24);
            let name = f.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            ui.cv.text_in(ui.f, PX, R::new(row.x + 4, row.y + 2, row.w - 30, 20), &name, t.text);
            ui.button(R::new(row.right() - 24, row.y + 1, 22, 22), "", "close", Tone::Ghost, false, true, Act::ShareRemove(i));
        }
    }
    ui.cv.pop_clip();

    // Transfers box
    let tr_box = R::new(rx, files_box.bottom() + 8, rw, body.bottom() - files_box.bottom() - 8);
    ui.cv.pxbox(tr_box, &BoxStyle::new(t.surface, t.border));
    let open_w = ui.w(PX, "папка загрузок") + 24;
    let clr_w = ui.w(PX, "история") + 22;
    ui.text_in(PX, R::new(tr_box.x + 10, tr_box.y + 6, tr_box.w - open_w - clr_w - 28, 20), "передачи", t.text_hi);
    ui.button(R::new(tr_box.right() - open_w - clr_w - 12, tr_box.y + 5, clr_w, 22), "история", "trash", Tone::Ghost, false, !sh.history.is_empty(), Act::ShareClearHistory);
    ui.button(R::new(tr_box.right() - open_w - 6, tr_box.y + 5, open_w, 22), "папка загрузок", "folder", Tone::Ghost, false, true, Act::ShareOpenDir);

    let tr_list = R::new(tr_box.x + 6, tr_box.y + 32, tr_box.w - 12, tr_box.h - 38);
    ui.cv.push_clip(tr_list);
    if sh.active_transfers.is_empty() && sh.history.is_empty() {
        ui.text_center(PX, R::new(tr_list.x, tr_list.y + 36, tr_list.w, 20), "нет передач", t.text_faint);
    } else {
        for (i, tr) in sh.active_transfers.iter().chain(sh.history.iter()).take(4).enumerate() {
            let row = R::new(tr_list.x, tr_list.y + i as i32 * 26, tr_list.w, 24);
            let arrow = if tr.upload { "↑" } else { "↓" };
            let sz = fmt_size(tr.total);
            let pct = if tr.total > 0 { ((tr.done * 100) / tr.total).min(100) } else { 100 };
            let st_txt = match tr.status {
                crate::share::TransferStatus::Active => format!("{pct}%"),
                crate::share::TransferStatus::Done => crate::i18n::tr("готово").to_string(),
                crate::share::TransferStatus::Failed => crate::i18n::tr("ошибка").to_string(),
                crate::share::TransferStatus::Cancelled => crate::i18n::tr("отменено").to_string(),
            };
            let line = format!("{arrow} {} · {sz} · {st_txt}", tr.name);
            let col = match tr.status {
                crate::share::TransferStatus::Active => t.text_hi,
                crate::share::TransferStatus::Done => t.ok,
                crate::share::TransferStatus::Failed => t.red,
                crate::share::TransferStatus::Cancelled => t.text_dim,
            };
            ui.cv.text_in(ui.f, PX, R::new(row.x + 4, row.y + 2, row.w - 8, 20), &line, col);
        }
    }
    ui.cv.pop_clip();

    ui.text_in(PX, R::new(fx, fr.h - 30, fr.w - 28, 20), "добавить файлы · остановить · Tab — программы", t.text_faint);
}

fn fmt_size(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1_048_576.0)
    } else {
        format!("{:.2} GB", n as f64 / 1_073_741_824.0)
    }
}

pub fn clip_text_qr(data: &[u8]) -> Result<crate::share::QrCode, &'static str> {
    let text = String::from_utf8_lossy(data);
    if text.len() > 192 {
        return Err("текст больше лимита QR");
    }
    crate::share::encode_qr(text.as_bytes()).map_err(|()| "не удалось создать QR-код")
}

pub fn attach_clip_bytes_to_share(share: &mut crate::share::ShareState, dir: &std::path::Path, data: &[u8], ext: &str) -> bool {
    if data.is_empty() || std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let path = dir.join(format!("clip-{:016x}.{ext}", yami_px::fnv(yami_px::FNV0, data)));
    // Reuse existing cached file so repeated shares do not duplicate image data.
    if !path.is_file() && std::fs::write(&path, data).is_err() {
        return false;
    }
    share.add_files(vec![path]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_text_qr_encodes_and_rejects_oversize() {
        let qr = clip_text_qr(b"hello clipboard").unwrap();
        assert!(qr.size >= 21);
        assert_eq!(qr.modules.len(), qr.size * qr.size);
        let too_long = vec![b'a'; 193];
        assert_eq!(clip_text_qr(&too_long).unwrap_err(), "текст больше лимита QR");
    }

    #[test]
    fn clip_image_attaches_to_share_without_duplication_or_autostart() {
        let dir = std::env::temp_dir().join(format!("yami-clip-share-test-{}", crate::share::gen_token()));
        let mut share = crate::share::ShareState::default();
        let png = b"\x89PNG\r\n\x1a\nfake";
        assert!(attach_clip_bytes_to_share(&mut share, &dir, png, "png"));
        assert!(attach_clip_bytes_to_share(&mut share, &dir, png, "png"));
        assert_eq!(share.files.len(), 1);
        assert!(!share.active);
        assert!(!attach_clip_bytes_to_share(&mut share, &dir, b"", "png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clip_pinned_image_missing_file_is_ignored_and_tab_switches() {
        let mut l = LState::default();
        l.open(LTab::Clip, &[], &[]);
        assert_eq!(l.tab, LTab::Clip);
        let missing = std::env::temp_dir().join("yami-does-not-exist-12345.png");
        let mut share = crate::share::ShareState::default();
        share.add_files(vec![missing]);
        assert!(share.files.is_empty());
        l.set_tab(LTab::Share, &[], &[]);
        assert_eq!(l.tab, LTab::Share);
        assert!(l.clip_qr.is_none());
    }
}
