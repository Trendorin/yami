// YAMI // шелл для niri на Rust: бар, лаунчер, буфер, питание, настройки, уведомления,
// трей, всплывашки (календарь, звук, сеть, bluetooth, система), виджеты рабочего стола,
// заголовки окон в режиме «стол», блокировка. Рисует сам, пикселями, в shm — без тулкитов.
//
//   yami-shell                запустить (так зовёт niri)
//   yami <команда>            то же, что yami-shell <команда>: справка — `yami help` (cmd.rs)

pub use yami_px::{font, gfx, icons, theme};

mod agents;
mod animtab;
mod app;
mod profile;
mod iconsets;
mod restyle;
mod displays;
mod audio;
mod bar;
mod bartab;
mod bright;
mod bttab;
mod sleep;
mod camera;
mod privacy;
mod catalog;
mod bundle;
mod calc;
mod cli;
mod clean;
mod click;
mod clock;
mod cmd;
mod dbus;
mod dirs;
mod doctor;
mod dock;
mod fonttab;
mod gameboost;
mod games;
mod i18n;
mod config;
mod control;
mod curtain;
mod ipc;
mod keys;
mod launcher;
mod lite;
mod lyrics;
mod lock;
mod login;
mod media;
mod menu;
mod msgs;
mod net;
mod niri;
mod niricfg;
mod notif;
mod need;
mod night;
mod pam;
mod panels;
mod input;
mod prefs;
mod proc;
mod render;
mod wayland;
mod plugins;
mod recolor;
mod recolor_apps;
mod recolor_art;
mod recolor_data;
mod recolor_math;
mod recolor_tpl;
mod reply;
mod report;
mod popups;
mod power;
mod session;
mod setclick;
mod settabs;
mod skwd;
mod settings;
mod shot;
mod shotcfg;
mod sys;
mod tick;
mod titles;
mod ui;
mod update;
mod updtab;
mod upinstall;
mod wall;
mod walls;
mod themetab;
mod weather;
mod welcome;
mod widgets;
mod wizinst;
mod wizpages;

fn main() {
    // sudo спрашивает пароль для установки модулей из первой настройки (wizinst.rs)
    if wizinst::askpass() {
        return;
    }
    // большие буферы (обои, скриншоты, обложки) — сразу через mmap и обратно системе при
    // освобождении: иначе glibc поднимает порог и держит их в куче нитей после каждого декода
    // (musl так не делает, и настроек этих у него нет)
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::mallopt(libc::M_MMAP_THRESHOLD, 256 << 10);
        libc::mallopt(libc::M_ARENA_MAX, 2);
    }
    let args: Vec<String> = std::env::args().collect();
    // `yami` (ссылка на этот бинарь) без команды — справка; `yami-shell` без команды — шелл
    let me = args.first().map(|a| a.rsplit('/').next().unwrap_or(a).to_string()).unwrap_or_default();
    if args.len() == 1 && me != "yami" {
        app::run();
        return;
    }
    std::process::exit(cmd::run(&args[1..], &me))
}
