// yami-setup — первая настройка: язык, вид, какие модули поставить. На весь экран, в стиле.
// Запускается шеллом при первом входе (метка ~/.local/state/yami/first-run) и потом по `yami setup`.
//
//   yami-setup
//   yami-setup shot [-o ПАПКА] [--lang en] [--plain] [--size ШxВ]   страницы в PNG без Wayland

mod app;
mod run;
mod wiz;

use std::path::PathBuf;
use wiz::{Page, W};
use yami_px::{font, gfx, icons, look, ui};

fn main() {
    if run::askpass() {
        return;
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") => return println!("yami-setup {}", env!("CARGO_PKG_VERSION")),
        Some("-h") | Some("--help") => return println!("yami-setup | shot [-o DIR] [--lang en] [--plain] [--size WxH]"),
        Some("shot") => std::process::exit(shot(&args[1..])),
        _ => {}
    }
    // вид мастера до выбора: как задумано, язык — системный
    let look = look::Look::from_json(&serde_json::json!({ "palette": "ash", "lang": "" }), Some(false));
    yami_px::i18n::set(&look.lang);
    std::process::exit(app::run(W::new(look)));
}

fn shot(args: &[String]) -> i32 {
    let mut out = PathBuf::from("shots");
    let mut lang = String::new();
    let mut plain = false;
    let (mut sw, mut sh) = (1280, 720);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-o" => out = it.next().map(PathBuf::from).unwrap_or(out),
            "--lang" => lang = it.next().cloned().unwrap_or_default(),
            "--plain" => plain = true,
            "--size" => {
                if let Some((a, b)) = it.next().and_then(|v| v.split_once('x')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))) {
                    (sw, sh) = (a, b);
                }
            }
            _ => {}
        }
    }
    let _ = std::fs::create_dir_all(&out);
    let mut w = W::new(look::Look::from_json(&serde_json::json!({ "palette": "ash", "lang": lang }), Some(false)));
    w.lang = lang.clone();
    w.plain = plain;
    w.look = look::Look::from_json(&w.style_json(), Some(false));
    w.look.animations = false;
    yami_px::i18n::set(&lang);
    let mut fonts = font::Fonts::with(w.look.font("setup"));
    let mut ic = icons::Icons::new("");
    let mut bad = 0;
    for (name, p) in [("hello", Page::Hello), ("look", Page::Look), ("modules", Page::Modules), ("restyle", Page::Restyle), ("confirm", Page::Confirm), ("install", Page::Install), ("failed", Page::Install), ("done", Page::Done)] {
        w.page = p;
        if p == Page::Install {
            w.step = Some((42, 100, "yami-files".into()));
            w.lines = vec!["Compiling yami-px v3.0.0".into(), "Compiling yami-files v3.0.0".into()];
            w.running = name == "install";
            w.failed = (name == "failed").then(|| "the disk is full: free a few gigabytes and run again · ~/.cache/yami/install.log".to_string());
        }
        let mut buf = vec![0u32; (sw * sh) as usize];
        let mut u = ui::Ui { cv: gfx::Canvas::new(&mut buf, sw, sh), f: &mut fonts, ic: &mut ic, t: w.look.theme, zones: vec![], ptr: None, down: false, anim: false, blink: true, tr: |s| s };
        wiz::draw(&mut u, &w, sw, sh);
        drop(u);
        let rgba: Vec<u8> = buf.iter().flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]).collect();
        if image::save_buffer(out.join(format!("{name}.png")), &rgba, sw as u32, sh as u32, image::ExtendedColorType::Rgba8).is_err() {
            bad += 1;
        }
    }
    i32::from(bad > 0)
}
