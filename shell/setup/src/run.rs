// Сама установка: мастер запускает install.sh --apply со своим выбором и читает его вывод. Пароль
// отдаётся только sudo и только через askpass: сокет в личной папке 0700, живёт, пока идёт установка.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub enum Ev {
    /// [k/n] шаг
    Step(u32, u32, String),
    Line(String),
    Done(Result<(), String>),
    WrongPassword,
}

pub const SOCK_ENV: &str = "YAMI_SETUP_ASKPASS";

/// режим askpass: sudo запустил нас, чтобы узнать пароль. true — это он и был
pub fn askpass() -> bool {
    let Some(p) = std::env::var_os(SOCK_ENV) else { return false };
    if let Ok(s) = std::os::unix::net::UnixStream::connect(PathBuf::from(p)) {
        let mut line = String::new();
        let _ = BufReader::new(s).read_line(&mut line);
        print!("{line}");
    }
    true
}

/// откуда ставить: папка, из которой шла первая установка
pub fn source_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("YAMI_SRC") {
        return PathBuf::from(d);
    }
    let state = yami_px::paths::state_dir().join("yami/source");
    std::fs::read_to_string(state).ok().map(|s| PathBuf::from(s.trim())).filter(|p| p.join("install.sh").exists()).unwrap_or_else(|| yami_px::paths::data_dir().join("yami/src"))
}

/// sudo спросит пароль? false — NOPASSWD или уже root
pub fn needs_password() -> bool {
    !Command::new("sudo").args(["-n", "true"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

fn serve_password(sock: PathBuf, pass: Arc<Mutex<String>>) -> Option<()> {
    let l = UnixListener::bind(sock).ok()?;
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let line = pass.lock().map(|p| p.clone()).unwrap_or_default();
            let _ = (&s).write_all(format!("{line}\n").as_bytes());
        }
    });
    Some(())
}

/// [3/8] шаг
fn step_of(s: &str) -> Option<(u32, u32, String)> {
    let i = s.find('[')?;
    let (nums, text) = s[i + 1..].split_once(']')?;
    let (k, n) = nums.split_once('/')?;
    Some((k.trim().parse().ok()?, n.trim().parse().ok()?, text.trim().to_string()))
}

fn plain_line(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            for d in it.by_ref() {
                if d.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// поставить: send получает ход установки из своей нити
pub fn start(modules: Vec<String>, restyle: Vec<String>, lang: String, mut password: String, send: impl Fn(Ev) + Send + 'static) {
    std::thread::spawn(move || {
        let src = source_dir();
        let script = src.join("install.sh");
        let mut c = Command::new("bash");
        c.arg(&script).args(["--apply", &modules.join(" "), "--restyle", &if restyle.is_empty() { "none".to_string() } else { restyle.join(" ") }]);
        if !lang.is_empty() {
            c.args(["--lang", &lang]);
        }
        c.current_dir(&src).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut dir = None;
        if password.is_empty() {
            if needs_password() {
                c.arg("--no-sudo");
            }
        } else {
            let Ok(d) = yami_px::paths::private_tmp("yami-setup") else { return send(Ev::Done(Err("tmp".into()))) };
            let sock = d.join("askpass");
            let pass = Arc::new(Mutex::new(password.clone()));
            if serve_password(sock.clone(), pass).is_none() {
                return send(Ev::Done(Err("askpass".into())));
            }
            let me = yami_px::paths::self_exe();
            let check = Command::new("sudo").args(["-k", "-A", "true"]).env("SUDO_ASKPASS", &me).env(SOCK_ENV, &sock).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
            if !check.is_ok_and(|s| s.success()) {
                let _ = std::fs::remove_dir_all(&d);
                yami_px::ui::wipe(&mut password);
                return send(Ev::WrongPassword);
            }
            c.env("SUDO_ASKPASS", &me).env(SOCK_ENV, &sock);
            dir = Some(d);
        }
        let mut child = match c.spawn() {
            Ok(ch) => ch,
            Err(e) => return send(Ev::Done(Err(e.to_string()))),
        };
        let (Some(out), err) = (child.stdout.take(), child.stderr.take()) else { return };
        if let Some(err) = err {
            std::thread::spawn(move || for _ in BufReader::new(err).lines() {});
        }
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let l = plain_line(&line);
            if l.is_empty() {
                continue;
            }
            match step_of(&l) {
                Some((k, n, t)) => send(Ev::Step(k, n, t)),
                None => send(Ev::Line(l)),
            }
        }
        let st = child.wait();
        if let Some(d) = dir {
            let _ = std::fs::remove_dir_all(d);
        }
        yami_px::ui::wipe(&mut password);
        send(Ev::Done(match st {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("install.sh: exit {}", s.code().unwrap_or(-1))),
            Err(e) => Err(e.to_string()),
        }));
    });
}
