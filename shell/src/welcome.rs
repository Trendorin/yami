// Первая настройка — окно настроек в другом виде: шаги слева, страница справа, внизу «назад»,
// клетки шагов и «дальше». Шаг — вкладка настроек (её draw как есть, всё применяется сразу) или своя
// страница: привет (язык, стиль, тема), niri (подключение, части yami, чужие бары и уведомления в
// конфиге), модули (поставить или убрать через install.sh, wizinst.rs), готово (главные клавиши).
// Открывается сама при первом входе (метка first-run: её кладёт установщик, а без настроек — сам
// шелл, если его поставили пакетом), потом — `yami setup` и кнопка во вкладке «о шелле». Закрыть
// можно в любой момент: метка снимается, выбранное остаётся.

use crate::app::{App, Modal, SKind, State};
use crate::font::{PX, PX24};
use crate::gfx::{BoxStyle, R};
use crate::settings::title;
use crate::ui::{Act, Cur, Field, Tone, Ui};
use crate::wizinst::Ev;

pub struct Step {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub draw: fn(&mut Ui, &State, R),
    /// у страницы свой список с прокруткой (клавиши)
    pub own_scroll: bool,
    /// после него в списке — промежуток (конец группы)
    pub gap: bool,
}

const fn step(id: &'static str, name: &'static str, icon: &'static str, draw: fn(&mut Ui, &State, R)) -> Step {
    Step { id, name, icon, draw, own_scroll: false, gap: false }
}

/// все шаги; id совпадают со вкладками настроек — их проверки (tab_probe) и прокрутка общие
const ALL: &[Step] = &[
    step("hello", "привет", "about", hello),
    step("color", "цвет", "drop", crate::settabs::tab_color),
    step("wall", "обои", "image", crate::wall::tab),
    step("look", "вид", "eye", crate::settabs::tab_look),
    Step { gap: true, ..step("fonts", "шрифты", "text", crate::fonttab::tab) },
    step("bar", "бар", "menu", crate::bartab::tab_bar),
    step("dock", "док", "tile", crate::settabs::tab_dock),
    step("widgets", "виджеты", "grid", crate::settabs::tab_widgets),
    step("windows", "окна", "float", crate::settabs::tab_windows),
    Step { own_scroll: true, gap: true, ..step("keys", "клавиши", "keys", crate::keys::tab) },
    step("niri", "niri", "window", niri_page),
    step("notif", "уведомления", "bell", crate::settabs::tab_notif),
    step("system", "система", "gear", crate::settabs::tab_system),
    step("agents", "службы", "bolt", crate::agents::tab),
    step("games", "игры", "play", crate::games::tab),
    Step { gap: true, ..step("login", "экран входа", "lock", crate::login::tab) },
    step("modules", "модули", "plus", modules_page),
    step("done", "готово", "check", done_page),
];

/// такой шаг есть (`yami setup STEP`)
pub fn has_step(id: &str) -> bool {
    ALL.iter().any(|s| s.id == id)
}

/// модуль первой настройки: id как у `install.sh --apply`
pub struct Module {
    pub id: &'static str,
    pub name: &'static str,
    /// что это (по-русски: переводится, как любой текст на экране)
    pub what: &'static str,
    /// стоит сейчас
    pub have: bool,
    /// отмечен: поставить (или оставить)
    pub want: bool,
}

const fn m(id: &'static str, name: &'static str, what: &'static str) -> Module {
    Module { id, name, what, have: false, want: false }
}

fn modules() -> Vec<Module> {
    vec![
        m("files", "yami-files", "файлы, диалоги, флешки и телефоны"),
        m("view", "yami-view", "картинки с быстрыми правками"),
        m("play", "yami-play", "видео и музыка (libmpv)"),
        m("shot", "yami-shot", "скриншоты, текст и QR"),
        m("agents", "yami-agents", "пароли, показ экрана, флешки"),
        m("login", "yami-greet", "свой экран входа"),
        m("games", "games", "GameMode и MangoHud"),
        m("fork", "yami-niri", "наш niri, сборка 5–25 минут"),
    ]
}

/// что из модулей стоит: проверяется при открытии настроек и после установки (вкладки модулей
/// без них прячутся)
#[derive(Clone, Copy, Default, Debug)]
pub struct Here {
    pub files: bool,
    pub view: bool,
    pub play: bool,
    pub shot: bool,
    pub agents: bool,
    pub greet: bool,
}

impl Here {
    pub fn probe() -> Here {
        let h = crate::proc::have;
        Here { files: h("yami-files"), view: h("yami-view"), play: h("yami-play"), shot: h("yami-shot"), agents: h("yami-agents"), greet: crate::login::installed() }
    }

    /// всё стоит: снимки панелей (`shot`) одинаковы на любой машине
    pub fn all() -> Here {
        Here { files: true, view: true, play: true, shot: true, agents: true, greet: true }
    }

    /// вкладку настроек (или шаг) показывать: её модуль стоит
    pub fn shows(&self, id: &str) -> bool {
        match id {
            "shots" => self.shot,
            "login" => self.greet,
            _ => true,
        }
    }
}

/// ход установки модулей
#[derive(Default)]
pub struct Inst {
    pub running: bool,
    pub step: Option<(u32, u32, String)>,
    pub lines: Vec<String>,
    pub failed: Option<String>,
    pub done: bool,
    pub wrong: bool,
    /// sudo спросит пароль: поле на странице
    pub pass: bool,
    pub sudo: bool,
}

pub struct Welcome {
    pub step: usize,
    /// дальше этого шага ещё не ходили: в списке без галочки
    pub seen: usize,
    /// номера шагов из ALL, что показываются на этой машине
    pub steps: Vec<usize>,
    pub mods: Vec<Module>,
    pub inst: Inst,
    pub pass: Field,
    pub others: Vec<crate::niricfg::Other>,
}

impl Welcome {
    pub fn cur(&self) -> &'static Step {
        &ALL[self.steps.get(self.step).copied().unwrap_or(0)]
    }

    fn changed(&self) -> bool {
        self.mods.iter().any(|m| m.have != m.want)
    }
}

fn has_steam() -> bool {
    let h = yami_px::paths::home();
    [".local/share/Steam", ".steam", ".var/app/com.valvesoftware.Steam"].iter().any(|d| h.join(d).is_dir()) || crate::proc::have("steam")
}

/// что из модулей стоит (games — по записи установщика, yami-niri — по сборке)
fn probe_mods(mods: &mut [Module]) {
    let comps = std::fs::read_to_string(yami_px::paths::state_dir().join("install.json")).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).and_then(|v| v["components"].as_str().map(str::to_string)).unwrap_or_default();
    let here = Here::probe();
    for m in mods.iter_mut() {
        m.have = match m.id {
            "files" => here.files,
            "view" => here.view,
            "play" => here.play,
            "shot" => here.shot,
            "agents" => here.agents,
            "login" => here.greet,
            "games" => comps.split_whitespace().any(|c| c == "games"),
            "fork" => yami_px::paths::home().join(".local/lib/yami-niri/current/niri").exists(),
            _ => false,
        };
        m.want = m.have;
    }
}

/// где страница: как у настроек, только снизу — полоса с кнопками
pub fn body_rect(w: i32, h: i32) -> R {
    let fr = R::new(0, 0, w - 6, h - 6);
    let side = 168;
    R::new(1 + side + 20, 48, fr.right() - side - 1 - 48, fr.h - 64 - FOOT)
}

const FOOT: i32 = 46;

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let Some(wz) = &st.welcome else { return };
    let t = ui.t;
    let fr = R::new(0, 0, w - 6, h - 6);
    ui.frame(fr, false);
    let n = wz.steps.len();
    let sub = crate::tf!("шаг {} из {}", "step {} of {}", "passo {} de {}", wz.step + 1, n);
    ui.header(R::new(1, 1, fr.w - 2, 30), "about", "первая настройка", &sub);
    ui.zone(R::new(1, 1, fr.w - 2, 30), Act::MDrag, Cur::Default);
    ui.ghost(R::new(fr.right() - 28, 5, 24, 22), "close", Act::CloseModal);
    // шаги: пройденные — с галочкой, текущий — с полосой акцента
    let side = R::new(1, 31, 168, h - 6 - 32);
    ui.cv.fill(side, t.panel.alpha(0.5));
    ui.cv.fill(R::new(side.right(), side.y, 1, side.h), t.border);
    ui.zone(side, Act::Block, Cur::Default);
    let mut y = side.y + 10;
    for (i, &k) in wz.steps.iter().enumerate() {
        let s = &ALL[k];
        let r = R::new(side.x + 8, y, side.w - 16, 24);
        ui.button(r, s.name, s.icon, Tone::Ghost, i == wz.step, true, Act::WStep(i));
        if i == wz.step {
            ui.cv.fill(R::new(r.x - 8, r.y, 3, r.h), t.red);
        } else if i < wz.seen {
            ui.cv.icon("check", r.right() - 11, r.y + 9, 1, t.ok.alpha(0.8));
        }
        y += 26;
        if s.gap {
            y += 8;
        }
    }
    // страница
    let body = body_rect(w, h);
    let cur = wz.cur();
    let off = if cur.own_scroll { 0 } else { crate::settings::scroll_of(st) };
    ui.zone(R::new(side.right() + 1, 31, fr.right() - side.right() - 2, body.bottom() + 8 - 31), Act::SScroll, Cur::Default);
    ui.cv.push_clip(R::new(body.x - 4, body.y - 4, body.w + 8, body.h + 8));
    ui.cv.reach = i32::MIN;
    (cur.draw)(ui, st, R::new(body.x, body.y - off, body.w, body.h));
    let full = if cur.own_scroll { body.h } else { (ui.cv.reach + off + 12 - body.y).max(body.h) };
    st.sscroll_h.set(full);
    ui.cv.pop_clip();
    if full > body.h {
        ui.cv.fill(R::new(body.right() + 19, body.y, 2, body.h), t.border.alpha(0.5));
        let th = (body.h * body.h / full).max(18);
        let ty = body.y + (body.h - th) * off / (full - body.h).max(1);
        ui.cv.fill(R::new(body.right() + 18, ty, 4, th), t.red);
    }
    // низ: назад · клетки шагов · дальше
    let fy = fr.bottom() - FOOT;
    let fx = side.right() + 1;
    ui.cv.fill(R::new(fx, fy, fr.right() - fx - 1, FOOT - 1), t.panel.alpha(0.35));
    ui.cv.fill(R::new(fx, fy, fr.right() - fx - 1, 1), t.border);
    ui.zone(R::new(fx, fy, fr.right() - fx - 1, FOOT - 1), Act::Block, Cur::Default);
    let by = fy + (FOOT - 28) / 2;
    ui.button(R::new(body.x, by, 112, 28), "назад", "left", Tone::Plain, false, wz.step > 0, Act::WBack);
    let last = wz.step + 1 >= n;
    let (label, icon) = if last { ("закрыть", "check") } else { ("дальше", "right") };
    ui.button(R::new(body.right() - 132, by, 132, 28), label, icon, Tone::Primary, false, true, Act::WNext);
    let cell = 8;
    let gap = 4;
    let dots = n as i32 * (cell + gap) - gap;
    let mut x = body.x + (body.w - dots) / 2;
    for i in 0..n {
        let c = if i == wz.step { t.red } else if i < wz.seen { t.ember.alpha(0.6) } else { t.border };
        ui.cv.fill(R::new(x, by + 10, cell, cell), c);
        ui.zone(R::new(x - 1, by + 4, cell + gap, 20), Act::WStep(i), Cur::Pointer);
        x += cell + gap;
    }
}

/// карточка выбора: значок, имя, две строки пояснения; выбранная — в рамке акцента
fn card(ui: &mut Ui, r: R, icon: &str, name: &str, desc: &str, on: bool, act: Act) {
    let t = ui.t;
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if on { t.surface_hi } else { t.surface }, if on || hov { t.red } else { t.border }).shadow(2));
    ui.cv.icon(icon, r.x + 14, r.y + 14, 3, if on { t.red } else { t.text_dim });
    ui.text_in(PX, R::new(r.x + 50, r.y + 10, r.w - 60, 20), name, if on { t.text_hi } else { t.text });
    ui.wrap(PX, R::new(r.x + 50, r.y + 32, r.w - 60, 36), desc, t.text_faint, 2);
    ui.zone(r, act, Cur::Pointer);
}

fn hello(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let s = &st.set;
    let mut y = b.y;
    ui.text_in(PX24, R::new(b.x, y, b.w, 30), "добро пожаловать в yami", t.text_hi);
    y += 38;
    let h = ui.wrap(PX, R::new(b.x, y, b.w, 60), "шелл для niri. Шаги слева — всё применяется сразу, и в любой шаг можно вернуться. Потом всё это живёт в настройках: Mod+S.", t.text_dim, 3);
    y += h + 16;
    y = title(ui, b.x, y, b.w, "язык");
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
    let bw = (b.w - 16) / 3;
    // названия языков — на своём языке, не переводятся
    for (i, (code, name)) in [("ru", "Русский"), ("en", "English"), ("pt-BR", "Português (Brasil)")].into_iter().enumerate() { // i18n-ok
        let r = R::new(b.x + i as i32 * (bw + 8), y, bw, 30);
        ui.button(r, "", "", Tone::Plain, cur == code, true, Act::WLang(code));
        ui.cv.text_in(ui.f, PX, R::new(r.x + 12, r.y, r.w - 24, r.h), name, if cur == code { t.text_hi } else { t.text });
    }
    y += 46;
    y = title(ui, b.x, y, b.w, "стиль");
    let cw = (b.w - 12) / 2;
    let plain = s.preset == "plain";
    card(ui, R::new(b.x, y, cw, 76), "grid", "пиксельный", "целые пиксели, ступенчатые углы, проявление растром", !plain, Act::WPlain(false));
    card(ui, R::new(b.x + cw + 12, y, cw, 76), "window", "нейтральный", "скругления, мягкие тени, обычные шрифты", plain, Act::WPlain(true));
    y += 92;
    y = title(ui, b.x, y, b.w, "тема");
    card(ui, R::new(b.x, y, cw, 64), "moon", "тёмная", "светлый текст на тёмном", !s.light, Act::SCycle("theme_dark"));
    card(ui, R::new(b.x + cw + 12, y, cw, 64), "sun", "светлая", "тёмный текст на светлом", s.light, Act::SCycle("theme_light"));
    y += 76;
    ui.wrap(PX, R::new(b.x, y, b.w, 40), "дальше — цвет, обои, вид и шрифты; потом бар, док, окна и клавиши; в конце — niri и модули", t.text_faint, 2);
}

fn niri_page(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let Some(w) = &st.welcome else { return };
    let nl = &st.niri_link;
    let half = (b.w - 20) / 2;
    let (lx, rx) = (b.x, b.x + half + 20);
    let mut y = title(ui, b.x, b.y, b.w, "конфиг niri остаётся твоим");
    y += ui.wrap(PX, R::new(b.x, y, b.w, 60), "yami не переписывает конфиг niri: в его начало встаёт одна строка include, и всё, что ниже неё, главнее. Конфига нет — yami положит свой стартовый.", t.text_dim, 3) + 10;
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
    y += 30;
    if nl.connected {
        ui.button(R::new(lx, y, half, 28), "убрать строку yami", "close", Tone::Plain, false, true, Act::WNiri("disconnect"));
    } else {
        ui.button(R::new(lx, y, half, 28), "подключить одной строкой", "plus", Tone::Primary, false, true, Act::WNiri("connect"));
        if nl.exists && !nl.niri_default {
            ui.button(R::new(rx, y, half, 28), "заменить стартовым yami", "reboot", Tone::Plain, false, true, Act::WNiri("replace"));
        }
    }
    y += 34;
    if nl.exists && !nl.connected && !nl.niri_default {
        y += ui.wrap(PX, R::new(b.x, y, b.w, 20), "заменить: твой файл уйдёт в копию рядом, config.kdl.bak-yami-…", t.text_faint, 1) + 6;
    }
    if !st.niri_note.is_empty() {
        y += ui.wrap(PX, R::new(b.x, y, b.w, 60), &st.niri_note, t.text_dim, 3) + 8;
    }
    y += 4;
    y = title(ui, b.x, y, b.w, "что приносит yami");
    y = crate::settabs::niri_parts(ui, st, b, y);
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

fn modules_page(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let Some(w) = &st.welcome else { return };
    let half = (b.w - 20) / 2;
    let mut y = title(ui, b.x, b.y, b.w, "модули");
    y += ui.wrap(PX, R::new(b.x, y, b.w, 40), "каждый — отдельная программа и работает без шелла. Отмеченные поставятся, снятые уйдут.", t.text_dim, 2) + 12;
    for (i, m) in w.mods.iter().enumerate() {
        let x = if i % 2 == 0 { b.x } else { b.x + half + 20 };
        let r = R::new(x, y, half, 40);
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
        if i % 2 == 1 {
            y += 48;
        }
    }
    if w.mods.len() % 2 == 1 {
        y += 48;
    }
    y += 4;
    if cfg!(feature = "package-managed") {
        ui.wrap(PX, R::new(b.x, y, b.w, 60), "yami поставлен пакетом: модули ставит менеджер пакетов — пакеты yami-files, yami-view, yami-play, yami-shot, yami-agents", t.text_dim, 3);
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
        ui.wrap(PX, R::new(b.x, y, b.w, 40), "отметь, что поставить или убрать; ставит install.sh, ход виден здесь", t.text_faint, 2);
        return;
    }
    let go = if inst.failed.is_some() { "ещё раз" } else { "применить" };
    if inst.pass {
        let f = st.field_focus == Some("w_pass");
        ui.field(R::new(b.x, y, half, 28), &w.pass, if inst.wrong { "неверный пароль, ещё раз" } else { "пароль sudo — для пакетов" }, f, true, Act::SField("w_pass"));
        ui.button(R::new(b.x + half + 20, y, half, 28), go, "down", Tone::Primary, false, !w.pass.text.is_empty(), Act::WInstall);
        y += 36;
        ui.wrap(PX, R::new(b.x, y, b.w, 40), "пароль уходит только sudo и нигде не сохраняется", t.text_faint, 2);
    } else {
        ui.button(R::new(b.x, y, half, 28), go, "down", Tone::Primary, false, true, Act::WInstall);
        if !inst.sudo {
            ui.wrap(PX, R::new(b.x + half + 20, y, half, 40), "sudo нет: пакеты потом одной командой — yami update need", t.text_faint, 2);
        }
    }
}

fn done_page(ui: &mut Ui, st: &State, b: R) {
    let t = ui.t;
    let mut y = b.y;
    ui.text_in(PX24, R::new(b.x, y, b.w, 30), "готово", t.text_hi);
    y += 38;
    y += ui.wrap(PX, R::new(b.x, y, b.w, 60), "yami настроен. Всё это живёт в настройках (Mod+S), а мастер снова откроет команда yami setup.", t.text_dim, 3) + 16;
    y = title(ui, b.x, y, b.w, "главные клавиши");
    let half = (b.w - 20) / 2;
    let ids: Vec<&str> = ["launcher", "settings", "terminal", "files", "clipboard", "shot", "walls", "lock", "overview", "hotkeys", "close", "power"]
        .into_iter()
        .filter(|id| *id != "files" || st.mods_here.files)
        .filter(|id| *id != "shot" || st.mods_here.shot)
        .collect();
    for (i, id) in ids.iter().enumerate() {
        let Some(bd) = crate::keys::BINDS.iter().find(|x| x.id == *id) else { continue };
        let x = if i % 2 == 0 { b.x } else { b.x + half + 20 };
        let key = crate::keys::key_of(&st.set, bd);
        let kw = (ui.w(PX, &key) + 16).max(64);
        ui.cv.pxbox(R::new(x, y, kw, 24), &BoxStyle::new(t.code_bg, t.border));
        ui.cv.text_in(ui.f, PX, R::new(x + 8, y, kw - 12, 24), &key, t.text_hi);
        ui.cv.text_in(ui.f, PX, R::new(x + kw + 10, y, half - kw - 10, 24), crate::keys::title_of(bd), t.text);
        if i % 2 == 1 {
            y += 32;
        }
    }
    if ids.len() % 2 == 1 {
        y += 32;
    }
    y += 6;
    ui.wrap(PX, R::new(b.x, y, b.w, 40), "все клавиши — в настройках → клавиши; их подсказка в niri — Mod+Shift+/", t.text_faint, 2);
}

impl App {
    /// открыть первую настройку на шаге (id шага; неизвестный — первый)
    pub fn welcome_open(&mut self, at: &str) {
        if self.st.locked {
            return;
        }
        self.st.mods_here = Here::probe();
        let here = self.st.mods_here;
        let steam = has_steam();
        let steps: Vec<usize> = (0..ALL.len()).filter(|&i| here.shows(ALL[i].id) && (ALL[i].id != "games" || steam)).collect();
        let mut mods = modules();
        probe_mods(&mut mods);
        let step = steps.iter().position(|&i| ALL[i].id == at).unwrap_or(0);
        let wz = Welcome { step, seen: step, steps, mods, inst: Inst { sudo: crate::wizinst::has_sudo(), ..Default::default() }, pass: Field::default(), others: vec![] };
        if self.st.modal != Some(Modal::Settings) {
            self.toggle_modal(Modal::Settings, false);
        }
        self.st.welcome = Some(wz);
        self.welcome_go(step);
    }

    fn welcome_go(&mut self, i: usize) {
        let Some(wz) = self.st.welcome.as_mut() else { return };
        let i = i.min(wz.steps.len().saturating_sub(1));
        wz.step = i;
        wz.seen = wz.seen.max(i);
        let id = wz.cur().id;
        self.st.stab = id;
        self.st.sscroll = (0, id);
        self.st.field_focus = None;
        self.keys_capture(None);
        match id {
            "niri" => {
                self.st.niri_link = crate::niricfg::status();
                let others = crate::niricfg::others();
                if let Some(wz) = self.st.welcome.as_mut() {
                    wz.others = others;
                }
            }
            "modules" => {
                // пароль спросим, только если sudo его хочет (sudo -n — короткий процесс)
                let pass = crate::wizinst::needs_password();
                if let Some(wz) = self.st.welcome.as_mut() {
                    wz.inst.pass = pass;
                }
            }
            _ => self.tab_probe(),
        }
        self.dirty(&SKind::Modal);
    }

    /// мастер закрывают (крестик, Esc, «закрыть»): метка первого входа снимается, всё выбранное — на диск
    pub fn welcome_end(&mut self) {
        if self.st.welcome.is_none() {
            return;
        }
        let _ = std::fs::remove_file(yami_px::paths::state_dir().join("first-run"));
        let j = self.st.set.to_json();
        let _ = crate::config::write_atomic(&crate::config::settings_path(), &j);
        self.set_saved = j;
        if let Some(wz) = self.st.welcome.as_mut() {
            yami_px::ui::wipe(&mut wz.pass.text);
        }
    }

    /// клики мастера; true — это был он
    pub fn welcome_click(&mut self, a: &Act) -> bool {
        if self.st.welcome.is_none() {
            return false;
        }
        let at = self.st.welcome.as_ref().map_or(0, |w| w.step);
        match a {
            Act::WStep(i) => self.welcome_go(*i),
            Act::WBack => self.welcome_go(at.saturating_sub(1)),
            Act::WNext => {
                let n = self.st.welcome.as_ref().map_or(0, |w| w.steps.len());
                if at + 1 >= n {
                    self.welcome_end();
                    self.close_modal();
                } else {
                    self.welcome_go(at + 1);
                }
            }
            Act::WLang(code) => {
                self.st.set.lang = code.to_string();
                self.settings_changed();
            }
            Act::WPlain(plain) => {
                if (self.st.set.preset == "plain") != *plain {
                    self.settings_click(&Act::SCycle("preset"));
                }
            }
            Act::WMod(i) => {
                if let Some(w) = self.st.welcome.as_mut() {
                    // пока ставится — выбор не меняется
                    if !w.inst.running {
                        if let Some(m) = w.mods.get_mut(*i) {
                            m.want = !m.want;
                        }
                    }
                    w.inst.failed = None;
                    w.inst.done = false;
                }
            }
            Act::WInstall => self.welcome_install(),
            Act::WNiri(op) => {
                let op = *op;
                let tx = self.tx.clone();
                self.save_now();
                std::thread::spawn(move || {
                    let r = match op {
                        "disconnect" => crate::niricfg::disconnect(),
                        "replace" => crate::niricfg::connect(crate::niricfg::Mode::Replace),
                        _ => crate::niricfg::connect(crate::niricfg::Mode::Auto),
                    };
                    let _ = tx.send(crate::app::Msg::NiriLink(crate::niricfg::status(), r.unwrap_or_else(|e| e)));
                });
            }
            Act::WOther(i) => {
                let Some(o) = self.st.welcome.as_ref().and_then(|w| w.others.get(*i)).cloned() else { return true };
                self.st.niri_note = match crate::niricfg::comment_out(&o) {
                    Ok(s) | Err(s) => s,
                };
                let others = crate::niricfg::others();
                if let Some(w) = self.st.welcome.as_mut() {
                    w.others = others;
                }
            }
            _ => return false,
        }
        self.dirty(&SKind::Modal);
        true
    }

    fn welcome_install(&mut self) {
        let Some(w) = self.st.welcome.as_mut() else { return };
        if w.inst.running || !w.changed() {
            return;
        }
        if w.inst.pass && w.pass.text.is_empty() {
            self.st.field_focus = Some("w_pass");
            return;
        }
        let list: Vec<String> = w.mods.iter().filter(|m| m.want).map(|m| m.id.to_string()).collect();
        let pass = std::mem::take(&mut w.pass.text);
        w.pass = Field::default();
        w.inst = Inst { running: true, pass: w.inst.pass, sudo: w.inst.sudo, ..Default::default() };
        self.st.field_focus = None;
        // установщик читает настройки с диска (что перекрашивать, язык): сначала они
        let j = self.st.set.to_json();
        let _ = crate::config::write_atomic(&crate::config::settings_path(), &j);
        self.set_saved = j;
        crate::wizinst::start(list, self.st.set.lang.clone(), pass, self.tx.clone());
    }

    /// ход установки из нити wizinst
    pub fn on_install(&mut self, e: Ev) {
        let Some(w) = self.st.welcome.as_mut() else { return };
        match e {
            Ev::Step(k, n, t) => w.inst.step = Some((k, n, t)),
            Ev::Line(s) => {
                w.inst.lines.push(s);
                let over = w.inst.lines.len().saturating_sub(8);
                w.inst.lines.drain(..over);
            }
            Ev::WrongPassword => {
                w.inst.running = false;
                w.inst.wrong = true;
                self.st.field_focus = Some("w_pass");
            }
            Ev::Done(r) => {
                w.inst.running = false;
                w.inst.done = r.is_ok();
                w.inst.failed = r.err();
                probe_mods(&mut w.mods);
                self.st.mods_here = Here::probe();
            }
        }
        self.dirty(&SKind::Modal);
    }

    /// раз в секунду: первый вход — мастер, как только есть экран и бар успел появиться
    pub fn welcome_tick(&mut self) {
        if !self.st.welcome_due || self.st.started.elapsed().as_millis() < 1500 || self.st.modal.is_some() || self.st.locked || self.output_names().is_empty() {
            return;
        }
        self.st.welcome_due = false;
        self.welcome_open("hello");
    }
}

/// для `yami-shell shot welcome:<шаг>[-состояние]`: мастер на шаге, модули и niri — выдуманные
pub fn mock(st: &mut State, arg: &str) {
    let (id, state) = arg.split_once('-').unwrap_or((arg, ""));
    let steps: Vec<usize> = (0..ALL.len()).filter(|&i| ALL[i].id != "login").collect();
    let step = steps.iter().position(|&i| ALL[i].id == id).unwrap_or(0);
    let mut mods = modules();
    for m in mods.iter_mut() {
        m.have = matches!(m.id, "files" | "shot" | "agents");
        m.want = m.have || m.id == "view";
    }
    mods[2].want = false;
    if let Some(m) = mods.iter_mut().find(|m| m.id == "agents") {
        m.want = false;
    }
    let mut inst = Inst { sudo: true, pass: state == "pass", ..Default::default() };
    match state {
        "busy" => {
            inst.running = true;
            inst.step = Some((42, 100, "yami-view: build from source".into()));
            inst.lines = vec!["Compiling yami-px v3.0.0".into(), "Compiling image v0.25.6".into(), "Compiling yami-view v3.0.0".into()];
        }
        "failed" => inst.failed = Some("the disk is full: free a few gigabytes and run again · ~/.cache/yami/install.log".into()),
        _ => {}
    }
    let others = if state == "others" {
        vec![
            crate::niricfg::Other { prog: "waybar", kind: "bar", line: String::new() },
            crate::niricfg::Other { prog: "mako", kind: "notif", line: String::new() },
        ]
    } else {
        vec![]
    };
    st.niri_link = crate::niricfg::Status { path: yami_px::paths::home().join(".config/niri/config.kdl"), exists: true, connected: state != "others", niri_default: false };
    st.niri_note = String::new();
    st.stab = ALL[steps[step]].id;
    st.sscroll = (0, st.stab);
    st.mods_here = Here { files: true, view: false, play: false, shot: true, agents: true, greet: false };
    st.welcome = Some(Welcome { step, seen: step, steps, mods, inst, pass: Field::default(), others });
}
