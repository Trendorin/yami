// yami-agents — службы рабочего стола одним фоновым процессом: окно пароля polkit и зашифрованных
// дисков, окно «что показать» и бэкенд показа экрана, автоподключение флешек и телефонов. Каждая
// часть выключается отдельно (agents.json). От шелла yami не зависит. Показ экрана нужен niri.
//
//   yami-agents                         запустить (второй экземпляр сразу выходит)
//   yami-agents status                  что включено и кто сейчас делает работу
//   yami-agents enable|disable ЧАСТЬ    polkit screencast automount automount-open
//   yami-agents --reload                перечитать настройки и перейти на новый бинарь
//   yami-agents shot [-o ПАПКА]         окна в PNG без Wayland

mod setup;
mod app;
mod automount;
mod cfg;
mod i18n;
mod notify;
mod polkit;
mod reload;
mod screen;
mod screencast;

use std::path::PathBuf;
use yami_px::{font, gfx, icons, look, paths};

const HELP: &str = "yami-agents                       run (a second instance exits)
yami-agents status                what is on, who does the job now
yami-agents enable|disable PART   polkit screencast automount automount-open
yami-agents --reload              reread the settings, move to a new binary
yami-agents install|uninstall|deps
yami-agents shot [-o DIR] [--lang en]

settings: ~/.config/yami/agents.json, look: ~/.config/yami/style.json and style.agents.json";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(c) = yami_px::install::dispatch(&setup::SPEC, &args) {
        std::process::exit(c);
    }
    match args.first().map(String::as_str) {
        Some("--version") => return println!("yami-agents {}", env!("CARGO_PKG_VERSION")),
        Some("-h") | Some("--help") => return println!("{HELP}"),
        Some("--reload") => std::process::exit(reload::signal()),
        Some("status") => std::process::exit(status()),
        Some("shot") => std::process::exit(shot(&args[1..])),
        Some(w @ ("enable" | "disable")) => std::process::exit(switch(w == "enable", args.get(1).map(String::as_str))),
        None => {}
        Some(other) => {
            eprintln!("yami-agents: unknown command: {other}\n{HELP}");
            std::process::exit(2);
        }
    }
    let Some(lock) = reload::single_instance() else { return };
    // SIGHUP блокируется до первой нити: иначе его получит кто попало
    reload::block_hangup();
    let look = look::Look::load("agents");
    i18n::set(&look.lang);
    std::process::exit(app::run(look, cfg::Cfg::load(), lock));
}

fn switch(on: bool, part: Option<&str>) -> i32 {
    let mut c = cfg::Cfg::load();
    let Some(slot) = part.and_then(|p| c.slot(p)) else {
        eprintln!("yami-agents: polkit, screencast, automount or automount-open");
        return 2;
    };
    *slot = on;
    c.save();
    reload::signal();
    0
}

/// владелец имени на шине, если он есть
fn owner(bus: &zbus::blocking::Connection, name: &'static str) -> Option<String> {
    let dbus = zbus::blocking::fdo::DBusProxy::new(bus).ok()?;
    let name = zbus::names::BusName::try_from(name).ok()?;
    dbus.get_name_owner(name).ok().map(|o| o.to_string())
}

fn status() -> i32 {
    let c = cfg::Cfg::load();
    let line = |name: &str, on: bool, who: String| println!("{name:<12} {:<4} {who}", if on { "on" } else { "off" });
    let running = reload::running_pid();
    let me = if running.is_some() { "yami-agents is running" } else { "yami-agents is not running" };
    line("polkit", c.polkit, me.into());
    let portal = zbus::blocking::Connection::session().ok().and_then(|b| owner(&b, screencast::NAME));
    line("screencast", c.screencast, portal.map_or("no portal backend on the bus".to_string(), |o| format!("portal backend {o}")));
    line("automount", c.automount, me.into());
    line("open mounted", c.automount_open, String::new());
    0
}

/// offscreen: окно пароля и окно выбора экрана
fn shot(args: &[String]) -> i32 {
    let mut out = PathBuf::from("shots");
    let mut lang = String::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" => out = it.next().map(PathBuf::from).unwrap_or(out),
            "--lang" => lang = it.next().cloned().unwrap_or_default(),
            _ => {}
        }
    }
    let mut look = look::Look::load("agents");
    look.animations = false;
    if !lang.is_empty() {
        look.lang = lang.clone();
    }
    i18n::set(&look.lang);
    i18n::audit(lang == "en" || yami_px::i18n::normalize(&lang) == "pt-BR");
    let _ = std::fs::create_dir_all(&out);
    let mut fonts = font::Fonts::with(look.font("agents"));
    let mut ic = icons::Icons::new("");
    let mut bad = 0;
    let reqs = polkit::mock();
    let pick = screen::mock();
    for name in ["auth", "pick"] {
        let (w, h) = if name == "auth" { polkit::SIZE } else { screen::size(&pick) };
        let mut buf = vec![0u32; (w * h) as usize];
        let mut ui = yami_px::ui::Ui { cv: gfx::Canvas::new(&mut buf, w, h), f: &mut fonts, ic: &mut ic, t: look.theme, zones: vec![], ptr: None, down: false, anim: false, blink: false, tr: i18n::tr };
        if name == "auth" {
            polkit::draw(&mut ui, &reqs, w, h);
        } else {
            screen::draw(&mut ui, &pick, w, h);
        }
        drop(ui);
        let rgba: Vec<u8> = buf.iter().flat_map(|p| {
            let a = (p >> 24) as u8;
            let un = |v: u8| if a == 0 { 0 } else { (v as u32 * 255 / a as u32).min(255) as u8 };
            [un((p >> 16) as u8), un((p >> 8) as u8), un(*p as u8), a]
        }).collect();
        let f = out.join(format!("{name}.png"));
        if image::save_buffer(&f, &rgba, w as u32, h as u32, image::ExtendedColorType::Rgba8).is_err() {
            bad += 1;
        }
        println!("{}", f.display());
    }
    let _ = paths::home();
    let missed = i18n::take_missed();
    for m in &missed {
        eprintln!("no translation: {m}");
    }
    i32::from(bad > 0 || !missed.is_empty())
}
