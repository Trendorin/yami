// Первая настройка — мастер по шагам: одна тема на экран, крупный заголовок, карточки с миниатюрами,
// вверху полоса шагов, внизу «назад» и «дальше». Всё применяется сразу. За кнопкой «все настройки»
// на шаге — соответствующие вкладки настроек целиком (их draw как есть), поэтому мастер не беднее
// настроек, но не похож на них. Страницы — wizpages.rs, установка модулей — wizinst.rs.
// Открывается сама при первом входе (метка first-run: её кладёт установщик, а без настроек — сам
// шелл, если его поставили пакетом), потом — `yami setup` и кнопка во вкладке «о шелле». Закрыть
// нечаянно нельзя: крестик и Esc спрашивают, а закрытый не пройдя мастер ждёт: настройки возвращают
// в него на тот же шаг, а метка хранит этот шаг до следующего входа. Метка снимается только в конце.

use crate::app::{App, Modal, SKind, State};
use crate::font::{self, PX, PX24};
use crate::gfx::{BoxStyle, R};
use crate::settings::{scroll_of, scrollbar, Tab};
use crate::ui::{Act, Cur, Field, Tone, Ui};
use crate::wizinst::Ev;
use crate::wizpages as page;
use std::time::{Duration, Instant};

pub struct Step {
    pub id: &'static str,
    /// к какой части относится: части разделены на полосе вверху
    pub chapter: &'static str,
    pub title: &'static str,
    pub sub: &'static str,
    pub draw: fn(&mut Ui, &State, R),
    /// вкладки настроек о том же, целиком: кнопка «все настройки»
    pub more: &'static [&'static str],
}

const fn step(id: &'static str, chapter: &'static str, title: &'static str, sub: &'static str, draw: fn(&mut Ui, &State, R), more: &'static [&'static str]) -> Step {
    Step { id, chapter, title, sub, draw, more }
}

const ALL: &[Step] = &[
    step("hello", "начало", "добро пожаловать в yami", "пара минут — и стол будет твоим. Всё применяется сразу, и всё потом можно поменять.", page::hello, &[]),
    step("style", "начало", "какой стиль?", "панели, окна и программы yami будут выглядеть так", page::style, &["look"]),
    step("color", "вид", "твой цвет", "весь стол и программы перекрасятся сразу", page::color, &["color"]),
    step("wall", "вид", "обои", "своя картинка или узор в цвет палитры", page::wall, &["wall"]),
    step("look", "вид", "пиксели и стекло", "насколько всё пиксельное и как выглядят панели", page::look, &["look", "fonts"]),
    step("anim", "вид", "анимации", "как открываются окна и панели", page::anim, &["anim"]),
    step("bar", "рабочий стол", "бар", "форма, край экрана и что на нём", page::bar, &["bar"]),
    step("dock", "рабочий стол", "док", "закреплённые и открытые программы", page::dock, &["dock"]),
    step("windows", "рабочий стол", "окна", "лента, как в niri, или обычный стол", page::windows, &["windows", "screens"]),
    step("widgets", "рабочий стол", "виджеты", "что лежит на рабочем столе", page::widgets, &["widgets"]),
    step("keys", "управление", "клавиши", "раскладки клавиатуры и главные сочетания", page::keys, &["keys"]),
    step("niri", "управление", "подключим к niri", "одна строка в его конфиге — и заработают клавиши, вид и правила окон", page::niri, &["system"]),
    step("modules", "система", "программы", "каждая — отдельная и работает без шелла", page::modules, &[]),
    step("services", "система", "уведомления и службы", "что шелл берёт на себя", page::services, &["notif", "agents"]),
    step("games", "система", "игры", "окна на весь экран, VRR и режим игры", crate::games::tab, &[]),
    step("login", "система", "экран входа", "как выглядит вход в систему", crate::login::tab, &[]),
    step("done", "готово", "готово", "yami настроен. Всё это живёт в настройках (Mod+S), а мастер снова откроет команда yami setup.", page::done, &[]),
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
        m("files", "yami-files", "файлы, диалоги, флешки"),
        m("view", "yami-view", "картинки с быстрыми правками"),
        m("play", "yami-play", "видео и музыка (libmpv)"),
        m("shot", "yami-shot", "скриншоты, текст и QR"),
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
    /// открыты «все настройки»: номер вкладки из Step::more
    pub more: Option<usize>,
    /// с какого мгновения показана страница: она появляется, а не вспыхивает
    pub since: Instant,
    /// открыт вопрос «выйти из настройки?»
    pub ask: bool,
    /// пройден до конца или закрыт насовсем: ждать его больше не надо
    pub finished: bool,
}

impl Welcome {
    pub fn cur(&self) -> &'static Step {
        &ALL[self.steps.get(self.step).copied().unwrap_or(0)]
    }

    pub fn changed(&self) -> bool {
        self.mods.iter().any(|m| m.have != m.want)
    }

    fn more_tab(&self) -> Option<&'static Tab> {
        self.more.and_then(|i| self.cur().more.get(i)).and_then(|id| crate::settings::find(id))
    }

    /// у открытой вкладки свой список с прокруткой (клавиши)
    pub fn own_scroll(&self) -> bool {
        self.more_tab().is_some_and(|t| t.own_scroll)
    }
}

/// метка «первая настройка не пройдена»; внутри — шаг, на котором остановились
fn marker() -> std::path::PathBuf {
    yami_px::paths::state_dir().join("first-run")
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
            "login" => here.greet,
            "games" => comps.split_whitespace().any(|c| c == "games"),
            "fork" => yami_px::paths::home().join(".local/lib/yami-niri/current/niri").exists(),
            _ => false,
        };
        m.want = m.have;
    }
}

/// ширина колонки, в которой всё стоит: окно шире, по бокам воздух
const COL: i32 = 640;
const FOOT: i32 = 54;

fn col_x(w: i32) -> i32 {
    (w - 6 - COL) / 2
}

/// где страница: под заголовком и пояснением; у «всех настроек» пояснения нет, места больше
pub fn body_rect(wz: &Welcome, w: i32, h: i32) -> R {
    let top = if wz.more.is_some() { 126 } else { 134 };
    R::new(col_x(w), top, COL, h - 6 - FOOT - top - 8)
}

/// полоса шагов: клетка на шаг, части разделены; пройденные — акцентом, открытые нажимаются
fn progress(ui: &mut Ui, wz: &Welcome, r: R) {
    let t = ui.t;
    let n = wz.steps.len() as i32;
    let parts = wz.steps.windows(2).filter(|p| ALL[p[0]].chapter != ALL[p[1]].chapter).count() as i32;
    let (gap, part_gap) = (3, 13);
    let cw = (r.w - (n - 1 - parts) * gap - parts * part_gap) / n.max(1);
    let mut x = r.x;
    for (i, &k) in wz.steps.iter().enumerate() {
        if i > 0 && ALL[wz.steps[i - 1]].chapter != ALL[k].chapter {
            x += part_gap - gap;
        }
        let c = if i == wz.step {
            t.red
        } else if i < wz.step {
            t.red.alpha(0.5)
        } else if i <= wz.seen {
            t.text_faint.alpha(0.6)
        } else {
            t.border
        };
        ui.cv.fill(R::new(x, r.y, cw, r.h), c);
        if i <= wz.seen {
            ui.zone(R::new(x - 1, r.y - 8, cw + gap, r.h + 16), Act::WStep(i), Cur::Pointer);
        }
        x += cw + gap;
    }
}

pub fn draw(ui: &mut Ui, st: &State, w: i32, h: i32) {
    let Some(wz) = &st.welcome else { return };
    let t = ui.t;
    let fr = R::new(0, 0, w - 6, h - 6);
    let x = col_x(w);
    ui.frame(fr, false);
    ui.zone(R::new(1, 1, fr.w - 2, 34), Act::MDrag, Cur::Default);
    ui.ghost(R::new(fr.right() - 30, 8, 24, 22), "close", Act::CloseModal);
    progress(ui, wz, R::new(x, 16, COL, 6));
    let cur = wz.cur();
    let nw = ui.text_in(PX, R::new(x, 38, COL / 2, 18), cur.chapter, t.red);
    let count = crate::tf!("шаг {} из {}", "step {} of {}", "passo {} de {}", wz.step + 1, wz.steps.len());
    ui.cv.text_in(ui.f, PX, R::new(x + nw + 12, 38, COL / 2, 18), &count, t.text_faint);
    ui.text_in(PX24, R::new(x, 58, COL - 80, 30), cur.title, t.text_hi);
    // начало и конец — с иероглифом, как на экране блокировки
    if matches!(cur.id, "hello" | "done") {
        ui.text(font::jp(64), x + COL - 66, 36, "闇", t.red.fade(0.75));
    }
    if wz.more.is_none() {
        ui.wrap(PX, R::new(x, 92, COL - 80, 40), cur.sub, t.text_dim, 2);
    } else {
        // несколько вкладок — переключатели; одна — её имя
        let mut bx = x;
        for (i, id) in cur.more.iter().enumerate() {
            let Some(tb) = crate::settings::find(id).filter(|tb| st.mods_here.shows(tb.id)) else { continue };
            let bw = ui.w(PX, tb.name) + 42;
            ui.button(R::new(bx, 92, bw, 24), tb.name, tb.icon, Tone::Ghost, wz.more == Some(i), true, Act::WMore(i as u8));
            bx += bw + 6;
        }
    }
    let body = body_rect(wz, w, h);
    let tab = wz.more_tab();
    let own = wz.own_scroll();
    let off = if own { 0 } else { scroll_of(st) };
    ui.zone(R::new(1, body.y - 6, fr.w - 2, body.h + 12), Act::SScroll, Cur::Default);
    ui.cv.push_clip(R::new(body.x - 4, body.y - 4, body.w + 8, body.h + 8));
    ui.cv.reach = i32::MIN;
    // страница появляется: сдвиг ступенями по 2 px и проявление
    let ms = if st.set.anim() { st.set.anim_ms("windows") * 0.7 } else { 0.0 };
    let k = if ms <= 0.0 { 1.0 } else { (wz.since.elapsed().as_secs_f32() * 1000.0 / ms).min(1.0) };
    if k < 1.0 {
        ui.anim = true;
    }
    let shift = (((1.0 - k) * (1.0 - k) * 16.0 / 2.0).round() as i32) * 2;
    let was = ui.cv.opacity;
    ui.cv.opacity = was * k;
    let area = R::new(body.x + shift, body.y - off, body.w, body.h);
    match tab {
        Some(tb) => (tb.draw)(ui, st, area),
        None => (cur.draw)(ui, st, area),
    }
    ui.cv.opacity = was;
    let full = if own { body.h } else { ui.cv.reach.saturating_add(off + 12 - body.y).max(body.h) };
    st.sscroll_h.set(full);
    ui.cv.pop_clip();
    scrollbar(ui, R::new(body.right() + 14, body.y, 4, body.h), body.h, full, off);
    // низ: назад · все настройки шага · дальше
    let fy = fr.bottom() - FOOT;
    ui.cv.fill(R::new(1, fy, fr.w - 2, 1), t.border);
    ui.zone(R::new(1, fy, fr.w - 2, FOOT - 1), Act::Block, Cur::Default);
    let by = fy + (FOOT - 30) / 2;
    ui.button(R::new(x, by, 124, 30), "назад", "left", Tone::Plain, false, wz.step > 0 || wz.more.is_some(), Act::WBack);
    let mid = R::new(x + 170, by, COL - 340, 30);
    if wz.more.is_some() {
        ui.button(mid, "вернуться к шагу", "left", Tone::Ghost, false, true, Act::WMore(u8::MAX));
    } else if !cur.more.is_empty() {
        ui.button(mid, "все настройки", "gear", Tone::Ghost, false, true, Act::WMore(0));
    }
    let last = wz.step + 1 >= wz.steps.len();
    let (label, icon) = if last { ("закрыть", "check") } else if wz.step == 0 { ("начать", "right") } else { ("дальше", "right") };
    ui.button(R::new(x + COL - 144, by, 144, 30), label, icon, Tone::Primary, false, true, Act::WNext);
    if wz.ask {
        leave_sheet(ui, fr);
    }
}

/// вопрос перед выходом: поверх всего окна, главная кнопка — остаться
fn leave_sheet(ui: &mut Ui, fr: R) {
    let t = ui.t;
    ui.cv.fill(R::new(1, 1, fr.w - 2, fr.h - 2), t.bg.alpha(0.82));
    ui.zone(fr, Act::Block, Cur::Default);
    let (w, h) = (470, 218);
    let r = R::new(fr.x + (fr.w - w) / 2, fr.y + (fr.h - h) / 2, w, h);
    ui.cv.pxbox(r, &BoxStyle::new(t.surface, t.red).shadow(3));
    ui.zone(r, Act::Block, Cur::Default);
    ui.text_in(PX24, R::new(r.x + 24, r.y + 18, r.w - 48, 30), "выйти из настройки?", t.text_hi);
    ui.wrap(PX, R::new(r.x + 24, r.y + 54, r.w - 48, 44), "всё выбранное сохранено. Вернуться можно в любой момент: откроется этот же шаг.", t.text_dim, 2);
    let b = r.x + 24;
    let bw = r.w - 48;
    ui.button(R::new(b, r.y + 104, bw, 32), "продолжить настройку", "right", Tone::Primary, false, true, Act::WLeave(0));
    let half = (bw - 10) / 2;
    ui.button(R::new(b, r.y + 146, half, 30), "вернусь позже", "close", Tone::Plain, false, true, Act::WLeave(1));
    ui.button(R::new(b + half + 10, r.y + 146, half, 30), "больше не показывать", "", Tone::Ghost, false, true, Act::WLeave(2));
    ui.wrap(PX, R::new(b, r.y + 184, bw, 22), "позже: кнопка настроек на баре или команда yami setup", t.text_faint, 1);
}

impl App {
    /// открыть первую настройку на шаге (id шага; неизвестный — первый)
    pub fn welcome_open(&mut self, at: &str) {
        if self.st.locked {
            return;
        }
        // ждёт незаконченный: вернуться в него, а не начинать заново (шаг задан — на него)
        if self.st.welcome_parked.is_some() {
            if self.st.modal != Some(Modal::Settings) {
                self.toggle_modal(Modal::Settings, false);
            }
            if at != "hello" {
                let i = self.st.welcome.as_ref().and_then(|w| w.steps.iter().position(|&k| ALL[k].id == at));
                if let Some(i) = i {
                    self.welcome_go(i);
                }
            }
            return;
        }
        self.st.mods_here = Here::probe();
        let here = self.st.mods_here;
        let steam = has_steam();
        let steps: Vec<usize> = (0..ALL.len()).filter(|&i| here.shows(ALL[i].id) && (ALL[i].id != "games" || steam)).collect();
        let mut mods = modules();
        probe_mods(&mut mods);
        let step = steps.iter().position(|&i| ALL[i].id == at).unwrap_or(0);
        let wz = Welcome { step, seen: step, steps, mods, inst: Inst { sudo: crate::wizinst::has_sudo(), ..Default::default() }, pass: Field::default(), others: vec![], more: None, since: Instant::now(), ask: false, finished: false };
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
        wz.more = None;
        wz.since = Instant::now();
        let id = wz.cur().id;
        // метка помнит шаг: следующий вход вернётся сюда
        let mark = marker();
        if mark.exists() {
            let _ = std::fs::write(&mark, id);
        }
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
                // пароль спросим, только если sudo его хочет: отдельный процесс, не на нити Wayland
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = tx.send(crate::app::Msg::Install(Ev::NeedsPassword(crate::wizinst::needs_password())));
                });
            }
            // шаг — сама вкладка настроек: её проверки
            "games" | "login" => self.tab_probe(),
            _ => {}
        }
        self.dirty(&SKind::Modal);
    }

    /// «все настройки» шага: вкладка настроек целиком; u8::MAX — обратно к шагу
    fn welcome_more(&mut self, i: u8) {
        let Some(wz) = self.st.welcome.as_mut() else { return };
        let cur = wz.cur();
        let tab = cur.more.get(i as usize).copied();
        wz.more = tab.map(|_| i as usize);
        wz.since = Instant::now();
        let id = tab.unwrap_or(cur.id);
        self.st.stab = id;
        self.st.sscroll = (0, id);
        self.st.field_focus = None;
        self.keys_capture(None);
        if tab.is_some() {
            self.tab_probe();
        }
    }

    /// мастер закрывают (крестик, Esc, «закрыть»): метка первого входа снимается, всё выбранное — на диск
    pub fn welcome_end(&mut self) {
        if self.st.welcome.is_none() {
            return;
        }
        let _ = std::fs::remove_file(marker());
        self.welcome_save();
        if let Some(wz) = self.st.welcome.as_mut() {
            wz.finished = true;
            yami_px::ui::wipe(&mut wz.pass.text);
        }
    }

    fn welcome_save(&mut self) {
        let j = self.st.set.to_json();
        let _ = crate::config::write_atomic(&crate::config::settings_path(), &j);
        self.set_saved = j;
    }

    /// крестик, Esc, щелчок мимо: сначала спросить
    pub fn welcome_ask(&mut self) {
        if let Some(w) = self.st.welcome.as_mut() {
            w.ask = true;
            self.st.field_focus = None;
            self.dirty(&SKind::Modal);
        }
    }

    /// окно настроек закрылось: мастер, который не пройден, ждёт (close_modal)
    pub fn welcome_park(&mut self) {
        let Some(mut w) = self.st.welcome.take() else { return };
        yami_px::ui::wipe(&mut w.pass.text);
        w.pass = Field::default();
        w.ask = false;
        if w.finished {
            return;
        }
        w.inst.pass = false;
        self.welcome_save();
        self.st.welcome_parked = Some(w);
        self.toast_note(&crate::tf!("первая настройка ждёт", "the first-run setup is waiting", "a configuração inicial está esperando"), &crate::tf!("вернуться: кнопка настроек на баре, откроется этот же шаг", "to go back: the settings button on the bar opens this same step", "para voltar: o botão de configurações na barra abre este mesmo passo"));
    }

    /// настройки открылись, а мастер ждал: вернуться в него на тот же шаг
    pub fn welcome_unpark(&mut self) {
        let Some(w) = self.st.welcome_parked.take() else { return };
        let at = w.step;
        self.st.welcome = Some(w);
        self.welcome_go(at);
    }

    /// клики мастера; true — это был он
    pub fn welcome_click(&mut self, a: &Act) -> bool {
        if self.st.welcome.is_none() {
            return false;
        }
        let at = self.st.welcome.as_ref().map_or(0, |w| w.step);
        match a {
            Act::WStep(i) => self.welcome_go(*i),
            Act::WBack => {
                // сначала закрываются «все настройки», потом — шаг назад
                if self.st.welcome.as_ref().is_some_and(|w| w.more.is_some()) {
                    self.welcome_more(u8::MAX);
                } else {
                    self.welcome_go(at.saturating_sub(1));
                }
            }
            Act::WMore(i) => self.welcome_more(*i),
            Act::WLeave(0) => {
                if let Some(w) = self.st.welcome.as_mut() {
                    w.ask = false;
                }
            }
            Act::WLeave(1) => {
                // close_modal оставит мастер ждать
                self.close_modal();
                return true;
            }
            Act::WLeave(_) => {
                self.welcome_end();
                self.close_modal();
                return true;
            }
            Act::WSet(k) => page::set(self, k),
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
                // niri validate — отдельный процесс: не на нити Wayland (список обновит NiriLink)
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let note = crate::niricfg::comment_out(&o).unwrap_or_else(|e| e);
                    let _ = tx.send(crate::app::Msg::NiriLink(crate::niricfg::status(), note));
                });
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
            Ev::NeedsPassword(p) => w.inst.pass = p,
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
        // метка помнит шаг, на котором остановились
        let saved = std::fs::read_to_string(marker()).unwrap_or_default();
        self.welcome_open(if has_step(saved.trim()) { saved.trim() } else { "hello" });
    }
}

/// для `yami-shell shot welcome:<шаг>[-состояние]`: мастер на шаге, модули и niri — выдуманные
pub fn mock(st: &mut State, arg: &str) {
    let (id, state) = arg.split_once('-').unwrap_or((arg, ""));
    let steps: Vec<usize> = (0..ALL.len()).filter(|&i| ALL[i].id != "login").collect();
    let step = steps.iter().position(|&i| ALL[i].id == id).unwrap_or(0);
    let mut mods = modules();
    for m in mods.iter_mut() {
        m.have = matches!(m.id, "files" | "shot" | "play");
        m.want = m.have || m.id == "view";
    }
    // одна программа поставится, одна уйдёт
    if let Some(m) = mods.iter_mut().find(|m| m.id == "play") {
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
    let more = (state == "more").then_some(0);
    st.welcome = Some(Welcome { step, seen: step, steps, mods, inst, pass: Field::default(), others, more, since: Instant::now().checked_sub(Duration::from_secs(9)).unwrap_or_else(Instant::now), ask: state == "leave", finished: false });
    if state == "more" {
        if let Some(id) = ALL[st.welcome.as_ref().map_or(0, |w| w.steps[w.step])].more.first() {
            st.stab = id;
            st.sscroll = (0, id);
        }
    }
}
