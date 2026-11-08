// Мастер первой настройки: страницы, их состояние и отрисовка. Язык и вид применяются сразу:
// весь мастер перекрашивается и меняет стиль, пока человек выбирает. Ставит не он, а install.sh
// (run.rs): здесь только выбор.
// i18n: inline pairs

use crate::run::Ev;
use std::time::Instant;
use yami_px::font::{self, PX, PX24};
use yami_px::gfx::{BoxStyle, C, R};
use yami_px::look::Look;
use yami_px::theme::PRESETS;
use yami_px::ui::{Cur, Field, Tone};

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
        m("files", "yami-files", 0, true, "файлы и диалог выбора файлов во всех программах", "files and the file dialog of every app", "arquivos e o diálogo de arquivos de todos os aplicativos"),
        m("view", "yami-view", 0, true, "картинки: мгновенно, с быстрыми правками", "pictures: instant, with quick edits", "imagens: instantâneo, com edições rápidas"),
        m("play", "yami-play", 0, true, "видео и музыка на libmpv", "video and music on libmpv", "vídeo e música com libmpv"),
        m("shot", "yami-shot", 0, true, "скриншоты: разметка, текст и QR со снимка", "screenshots: markup, text and QR from a shot", "capturas: anotações, texto e QR"),
        m("agents", "yami-agents", 1, true, "окна паролей, выбор экрана для трансляций, автоподключение", "password windows, the screen-share picker, automount", "janelas de senha, seleção de tela, montagem automática"),
        m("login", "yami-greet", 1, true, "свой экран входа вместо чужого", "our own login screen", "nossa própria tela de login"),
        m("drives", "drives", 1, true, "udisks2 и gvfs: флешки, телефоны, сетевые папки", "udisks2 and gvfs: drives, phones, network shares", "udisks2 e gvfs: pendrives, telefones, rede"),
        m("apps", "apps", 2, true, "kitty, fastfetch, pavucontrol, btop", "kitty, fastfetch, pavucontrol, btop", "kitty, fastfetch, pavucontrol, btop"),
        m("tools", "tools", 2, true, "ночной свет, распознавание текста и QR", "night light, text and QR recognition", "luz noturna, reconhecimento de texto e QR"),
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
    /// sudo спрашивать не будет (NOPASSWD)
    pub no_password: bool,
    pub lines: Vec<String>,
    pub step: Option<(u32, u32, String)>,
    pub failed: Option<String>,
    pub running: bool,
    pub wrong: bool,
}

impl W {
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
            lines: vec![],
            step: None,
            failed: None,
            running: false,
            wrong: false,
        }
    }

    /// настройки вида, которые мастер выбрал: для него самого и для файла настроек шелла
    pub fn style_json(&self) -> serde_json::Value {
        let reveal = if self.plain { "fade" } else { "dither" };
        serde_json::json!({
            "preset": if self.plain { "plain" } else { "yami" },
            "accent": format!("#{:06x}", self.accent),
            "light": self.light,
            "lang": self.lang,
            "reveal": reveal,
            "iconPixel": !self.plain,
            "glass": if self.plain { 1.0 } else { 0.55 },
        })
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

fn step_index(p: Page) -> usize {
    STEPS.iter().position(|s| *s == p).unwrap_or(STEPS.len())
}

pub fn next_of(p: Page) -> Option<Page> {
    STEPS.get(step_index(p) + 1).copied()
}

pub fn prev_of(p: Page) -> Option<Page> {
    step_index(p).checked_sub(1).and_then(|i| STEPS.get(i)).copied()
}

const CARD_W: i32 = 880;
const CARD_H: i32 = 560;

/// вся страница: фон, карточка по центру, содержимое, кнопки
pub fn draw(ui: &mut Ui, w: &W, sw: i32, sh: i32) {
    let t = ui.t;
    ui.cv.fill(R::new(0, 0, sw, sh), t.bg);
    backdrop(ui, w, sw, sh);
    let card = R::new((sw - CARD_W) / 2, (sh - CARD_H) / 2, CARD_W, CARD_H);
    ui.cv.pxbox(card, &BoxStyle::new(t.panel, t.border).shadow(6));
    ui.zone(card, Act::Block, Cur::Default);
    let body = card.inset(36);
    dots(ui, w, card);
    match w.page {
        Page::Hello => hello(ui, w, body),
        Page::Look => look_page(ui, w, body),
        Page::Modules => modules_page(ui, w, body),
        Page::Restyle => restyle_page(ui, w, body),
        Page::Confirm => confirm(ui, w, body),
        Page::Install => install(ui, w, body),
        Page::Done => done(ui, w, body),
    }
    let ms = w.reveal.elapsed().as_millis() as f32;
    let dur = w.look.dur(260.0);
    if dur > 0.0 && ms < dur {
        ui.cv.dissolve_rect(card, ms / dur, 3);
        ui.anim = true;
    }
}

/// за карточкой: у пиксельного вида — 闇 клетками, у нейтрального — ровный фон
fn backdrop(ui: &mut Ui, w: &W, sw: i32, sh: i32) {
    if w.plain {
        return;
    }
    let t = ui.t;
    let face = font::jp(240);
    let c = t.red.alpha(0.09);
    ui.text(face, sw - 300, sh - 330, "闇", c); // i18n-ok
    for y in (0..sh).step_by(4) {
        ui.cv.fill(R::new(0, y, sw, 1), C(0xff000000).alpha(0.10));
    }
}

/// где мы: точка на страницу
fn dots(ui: &mut Ui, w: &W, card: R) {
    if step_index(w.page) >= STEPS.len() {
        return;
    }
    let t = ui.t;
    let cur = step_index(w.page) as i32;
    let n = STEPS.len() as i32;
    let x0 = card.x + (card.w - (n * 22 - 8)) / 2;
    for i in 0..n {
        let r = R::new(x0 + i * 22, card.bottom() - 26, if i == cur { 22 } else { 8 }, 6);
        let r = if i > cur { R::new(r.x + 14, r.y, 8, 6) } else { r };
        ui.cv.notched(r, 1, if i == cur { t.red } else if i < cur { t.dim } else { t.border });
    }
}

fn head(ui: &mut Ui, b: R, title: &str, sub: &str) -> i32 {
    let t = ui.t;
    ui.cv.text_in(ui.f, PX24, R::new(b.x, b.y, b.w, 34), title, t.text);
    ui.cv.text_wrap(ui.f, PX, R::new(b.x, b.y + 38, b.w, 44), sub, t.text_dim, 2);
    b.y + 90
}

/// кнопки внизу карточки
fn nav(ui: &mut Ui, w: &W, b: R, next: &str, ok: bool) {
    let y = b.bottom() - 34;
    if prev_of(w.page).is_some() {
        ui.button(R::new(b.x, y, 130, 34), l("назад", "back", "voltar"), "left", Tone::Plain, false, true, Act::Back);
    }
    ui.button(R::new(b.right() - 180, y, 180, 34), next, "check", Tone::Primary, false, ok, Act::Next);
}

fn hello(ui: &mut Ui, w: &W, b: R) {
    let t = ui.t;
    let face = font::px(48);
    ui.cv.text_in(ui.f, face, R::new(b.x, b.y + 8, b.w, 60), l("Добро пожаловать", "Welcome", "Bem-vindo"), t.text);
    ui.cv.text_wrap(ui.f, PX, R::new(b.x, b.y + 84, b.w, 70), l("Рабочий стол уже работает. Пара вопросов — и мастер поставит только то, что ты выберешь. Всё можно поменять потом в настройках.", "The desktop is already running. A few questions, and the setup installs only what you pick. Everything can be changed later in the settings.", "A área de trabalho já está funcionando. Algumas perguntas, e a configuração instala só o que você escolher. Tudo pode ser alterado depois nas configurações."), t.text_dim, 3);
    let y = b.y + 176;
    ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w, 20), l("язык", "language", "idioma"), t.text_faint);
    let bw = (b.w - 2 * 12) / 3;
    for (i, (id, name)) in [("ru", "Русский"), ("en", "English"), ("pt-BR", "Português")].into_iter().enumerate() {
        let on = yami_px::i18n::normalize(&w.lang) == id;
        ui.button(R::new(b.x + i as i32 * (bw + 12), y + 28, bw, 56), name, "", Tone::Plain, on, true, Act::Lang(id));
    }
    let bottom = b.bottom() - 34;
    ui.button(R::new(b.x, bottom, 240, 34), l("пропустить настройку", "skip the setup", "pular a configuração"), "", Tone::Ghost, false, true, Act::Skip);
    ui.button(R::new(b.right() - 180, bottom, 180, 34), l("начать", "start", "começar"), "right", Tone::Primary, false, true, Act::Next);
}

/// миниатюра вида: окно с панелью и кнопками, нарисованное этим стилем
fn look_card(ui: &mut Ui, r: R, title: &str, sub: &str, plain: bool, on: bool) {
    let t = ui.t;
    let was = ui.cv.style;
    let style = if plain { yami_px::style::Style::PLAIN } else { yami_px::style::Style::PIXEL };
    let hov = ui.hover(r);
    ui.cv.pxbox(r, &BoxStyle::new(if on { t.selection.alpha(0.35) } else { t.surface }, if on { t.ember } else if hov { t.red } else { t.border }));
    ui.cv.style = style;
    let win = R::new(r.x + 20, r.y + 18, r.w - 40, 96);
    ui.cv.pxbox(win, &BoxStyle::new(t.panel, t.border).shadow(3));
    ui.cv.fill(R::new(win.x + 1, win.y + 22, win.w - 2, 1), t.border);
    for i in 0..3 {
        ui.cv.notched(R::new(win.x + 10 + i * 14, win.y + 8, 8, 8), 2, if i == 0 { t.red } else { t.dim });
    }
    ui.cv.pxbox(R::new(win.x + 12, win.y + 36, 82, 24), &BoxStyle::new(t.red, t.ember).bevel(true));
    ui.cv.pxbox(R::new(win.x + 104, win.y + 36, 82, 24), &BoxStyle::new(t.surface_hi, t.border).bevel(true));
    ui.cv.gauge(R::new(win.x + 12, win.y + 72, win.w - 24, 8), 16, 0.62, t.red, t.surface_hi, 2);
    ui.cv.style = was;
    ui.cv.text_in(ui.f, PX, R::new(r.x + 20, r.y + 122, r.w - 40, 20), title, t.text);
    ui.cv.text_in(ui.f, PX, R::new(r.x + 20, r.y + 142, r.w - 40, 20), sub, t.text_faint);
    ui.zone(r, Act::Preset(plain), Cur::Pointer);
}

fn look_page(ui: &mut Ui, w: &W, b: R) {
    let t = ui.t;
    let mut y = head(ui, b, l("Как будет выглядеть", "How it will look", "Como vai ficar"), l("Вид сразу применяется к этому окну. Его можно настроить вплоть до каждого цвета: style.json.", "The look applies to this window right away. Every colour can be changed later: style.json.", "A aparência é aplicada a esta janela na hora. Cada cor pode ser alterada depois: style.json."));
    let cw = (b.w - 16) / 2;
    look_card(ui, R::new(b.x, y, cw, 172), l("пиксели", "pixels", "pixels"), l("как задумано: фаски, ступени, Tiny5", "the original: notches, steps, Tiny5", "o original: chanfros, degraus, Tiny5"), false, !w.plain);
    look_card(ui, R::new(b.x + cw + 16, y, cw, 172), l("нейтральный", "neutral", "neutro"), l("скругления, системный шрифт, мягкие тени", "rounded corners, the system font, soft shadows", "cantos arredondados, fonte do sistema"), true, w.plain);
    y += 188;
    ui.cv.text_in(ui.f, PX, R::new(b.x, y, 100, 28), l("акцент", "accent", "destaque"), t.text_dim);
    let mut x = b.x + 104;
    for p in PRESETS {
        let r = R::new(x, y + 2, 24, 24);
        let c = C::hex(p.accent);
        if w.accent == p.accent {
            ui.cv.outline(R::new(r.x - 3, r.y - 3, r.w + 6, r.h + 6), t.text, 2);
        }
        ui.cv.notched(r, 2, c);
        ui.zone(r, Act::Accent(p.accent), Cur::Pointer);
        x += 34;
    }
    let tw = 96;
    ui.button(R::new(b.right() - 2 * tw - 8, y, tw, 28), l("тёмная", "dark", "escuro"), "", Tone::Plain, !w.light, true, Act::Light(false));
    ui.button(R::new(b.right() - tw, y, tw, 28), l("светлая", "light", "claro"), "", Tone::Plain, w.light, true, Act::Light(true));
    nav(ui, w, b, l("дальше", "next", "avançar"), true);
}

fn modules_page(ui: &mut Ui, w: &W, b: R) {
    let t = ui.t;
    let mut y = head(ui, b, l("Что поставить", "What to install", "O que instalar"), l("Модули независимы: без любого из них шелл работает, а поставить или убрать можно и потом — yami setup.", "Modules are independent: the shell runs without any of them, and you can add or remove them later: yami setup.", "Os módulos são independentes: o shell funciona sem eles, e dá para mudar depois: yami setup."));
    let col = (b.w - 20) / 2;
    let titles = [l("программы", "apps", "aplicativos"), l("система", "system", "sistema"), l("дополнительно", "extras", "extras")];
    let mut rows = [0i32; 2];
    for g in 0..3u8 {
        let c = usize::from(g == 2 || g == 1 && w.mods.iter().filter(|m| m.group == 0).count() > 5);
        let c = if g == 0 { 0 } else { c.max(usize::from(g > 0)) };
        let x = b.x + c as i32 * (col + 20);
        let mut ry = y + rows[c];
        ui.cv.text_in(ui.f, PX, R::new(x, ry, col, 20), titles[g as usize], t.text_faint);
        ry += 24;
        for (i, m) in w.mods.iter().enumerate().filter(|(_, m)| m.group == g) {
            ui.toggle(R::new(x, ry, col, 40), m.name, l(m.ru, m.en, m.pt), m.on, Act::Module(i));
            ry += 42;
        }
        rows[c] = ry - y + 8;
    }
    y += rows[0].max(rows[1]);
    let _ = y;
    nav(ui, w, b, l("дальше", "next", "avançar"), true);
}

fn restyle_page(ui: &mut Ui, w: &W, b: R) {
    let y = head(ui, b, l("Что перекрасить", "What to restyle", "O que recolorir"), l("Другие программы получат твою палитру. Снимешь галочку — yami их не тронет.", "Other programs get your palette. Untick one and yami leaves it alone.", "Outros programas recebem sua paleta. Desmarque e o yami não mexe neles."));
    let col = (b.w - 2 * 16) / 3;
    for (i, (_, name)) in RESTYLE.iter().enumerate() {
        let (cx, cy) = (i as i32 % 3, i as i32 / 3);
        ui.toggle(R::new(b.x + cx * (col + 16), y + cy * 46, col, 40), name, "", w.restyle[i], Act::Restyle(i));
    }
    nav(ui, w, b, l("дальше", "next", "avançar"), true);
}

fn confirm(ui: &mut Ui, w: &W, b: R) {
    let t = ui.t;
    let mut y = head(ui, b, l("Всё готово к установке", "Ready to install", "Pronto para instalar"), l("Модули собираются из исходников: обычно это несколько минут. Можно не ждать и работать дальше.", "The modules are built from source, usually a few minutes. You can keep working meanwhile.", "Os módulos são compilados do código-fonte, em geral alguns minutos. Você pode continuar usando."));
    let picked: Vec<&str> = w.mods.iter().filter(|m| m.on).map(|m| m.name).collect();
    let text = if picked.is_empty() { l("только шелл: ничего не добавлять", "only the shell: add nothing", "só o shell: não adicionar nada").to_string() } else { picked.join(" · ") };
    ui.cv.pxbox(R::new(b.x, y, b.w, 70), &BoxStyle::new(t.surface, t.border));
    ui.cv.text_wrap(ui.f, PX, R::new(b.x + 14, y + 10, b.w - 28, 50), &text, t.text, 2);
    y += 90;
    if !w.no_password {
        ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w, 20), l("пароль администратора — для пакетов (пусто: только домашняя папка)", "administrator password, for the packages (empty: home folder only)", "senha de administrador, para os pacotes (vazio: só a pasta pessoal)"), t.text_dim);
        y += 26;
        ui.field(R::new(b.x, y, b.w, 34), &w.field, l("пароль", "password", "senha"), true, true, Act::Pass);
        y += 42;
        if w.wrong {
            ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w, 20), l("неверный пароль", "wrong password", "senha incorreta"), t.ember);
        }
    }
    nav(ui, w, b, l("установить", "install", "instalar"), true);
}

fn install(ui: &mut Ui, w: &W, b: R) {
    let t = ui.t;
    let title = if w.failed.is_some() { l("Не получилось", "It did not work", "Não deu certo") } else { l("Устанавливаю", "Installing", "Instalando") };
    let mut y = head(ui, b, title, l("Окно можно не закрывать: рабочий стол продолжает работать.", "You can leave this open: the desktop keeps working.", "Pode deixar aberto: a área de trabalho continua funcionando."));
    if let Some((k, n, s)) = &w.step {
        ui.cv.text_in(ui.f, PX, R::new(b.x, y, b.w - 80, 22), s, t.text);
        ui.text_right(PX, R::new(b.x, y, b.w, 22), &format!("{k}/{n}"), t.text_faint);
        y += 28;
        ui.cv.gauge(R::new(b.x, y, b.w, 12), 40, *k as f32 / (*n).max(1) as f32, t.red, t.surface_hi, 2);
        y += 26;
    }
    if w.running {
        ui.anim = true;
    }
    let log = R::new(b.x, y, b.w, b.bottom() - 44 - y);
    ui.cv.pxbox(log, &BoxStyle::new(t.code_bg, t.border));
    ui.cv.push_clip(log.inset(8));
    for (i, line) in w.lines.iter().enumerate() {
        ui.cv.text(ui.f, PX, log.x + 10, log.y + 8 + i as i32 * 18, line, t.text_dim);
    }
    ui.cv.pop_clip();
    if let Some(why) = &w.failed {
        ui.cv.text_in(ui.f, PX, R::new(b.x, b.bottom() - 34, b.w - 200, 34), why, t.ember);
        ui.button(R::new(b.right() - 180, b.bottom() - 34, 180, 34), l("назад", "back", "voltar"), "left", Tone::Plain, false, true, Act::Back);
    }
}

fn done(ui: &mut Ui, _w: &W, b: R) {
    let t = ui.t;
    let face = font::px(48);
    ui.cv.text_in(ui.f, face, R::new(b.x, b.y + 8, b.w, 60), l("Готово", "All set", "Tudo pronto"), t.text);
    ui.cv.text_wrap(ui.f, PX, R::new(b.x, b.y + 80, b.w, 40), l("Вот что стоит запомнить. Поменять клавиши можно в настройках.", "Here is what is worth remembering. Keys can be changed in the settings.", "Isto vale lembrar. As teclas podem ser alteradas nas configurações."), t.text_dim, 2);
    let rows = [("Mod+Space", l("запуск программ", "launcher", "lançador")), ("Mod+S", l("настройки", "settings", "configurações")), ("Mod+V", l("буфер обмена", "clipboard", "área de transferência")), ("Mod+E", l("файлы", "files", "arquivos")), ("Print", l("скриншот", "screenshot", "captura de tela")), ("Mod+Shift+/", l("все клавиши", "all keys", "todas as teclas"))];
    let mut y = b.y + 136;
    for (k, v) in rows {
        ui.cv.pxbox(R::new(b.x, y, 150, 30), &BoxStyle::new(t.surface_hi, t.border).bevel(true));
        ui.cv.text_center(ui.f, PX, R::new(b.x, y, 150, 30), k, t.text);
        ui.cv.text_in(ui.f, PX, R::new(b.x + 170, y, b.w - 170, 30), v, t.text_dim);
        y += 40;
    }
    ui.button(R::new(b.right() - 180, b.bottom() - 34, 180, 34), l("начать работу", "start working", "começar a usar"), "check", Tone::Primary, false, true, Act::Finish);
}
