// Модули из первой настройки: шелл запускает install.sh --apply со списком и читает его вывод
// ([k/n] шаги, строки «! …» — почему не вышло). Пароль уходит только sudo и только через askpass:
// на время установки есть личная папка 0700 с сокетом и ссылкой `yami-askpass` на наш бинарь, её
// имя называется в SUDO_ASKPASS. Только запуск под этим именем отдаёт пароль: установщик зовёт
// `yami-shell` десятки раз (recolor, stamp), и никакой из этих вызовов askpass'ом стать не может.
// Своей копии исходников нет — клонируется заново.

use crate::app::Msg;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum Ev {
    /// [k/n] шаг
    Step(u32, u32, String),
    Line(String),
    Done(Result<(), String>),
    WrongPassword,
    /// sudo спросит пароль: на странице нужно поле
    NeedsPassword(bool),
}

/// имя ссылки на наш бинарь: под ним sudo запускает нас как askpass
const ASKPASS_NAME: &str = "yami-askpass";
const REPO: &str = "https://github.com/Trendorin/yami.git";

/// sudo запустил нас, чтобы узнать пароль: true — это он и был, больше ничего делать не надо
pub fn askpass() -> bool {
    let Some(arg0) = std::env::args_os().next() else { return false };
    let me = Path::new(&arg0);
    if me.file_name().is_none_or(|n| n != ASKPASS_NAME) {
        return false;
    }
    if let Ok(s) = UnixStream::connect(me.with_file_name("askpass")) {
        let mut line = String::new();
        let _ = BufReader::new(s).read_line(&mut line);
        print!("{line}");
    }
    true
}

/// пароль на время установки: папка 0700 с сокетом и ссылкой; всё уходит вместе с этим значением
struct Askpass {
    dir: PathBuf,
    pass: Arc<Mutex<String>>,
    stop: Arc<AtomicBool>,
}

impl Askpass {
    fn start(password: &str) -> Option<Askpass> {
        let dir = yami_px::paths::private_tmp("yami-askpass").ok()?;
        let a = Askpass { dir, pass: Arc::new(Mutex::new(password.to_string())), stop: Arc::new(AtomicBool::new(false)) };
        std::os::unix::fs::symlink(yami_px::paths::self_exe(), a.program()).ok()?;
        let l = UnixListener::bind(a.dir.join("askpass")).ok()?;
        let (pass, stop) = (a.pass.clone(), a.stop.clone());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let line = pass.lock().map(|p| p.clone()).unwrap_or_default();
                let _ = (&s).write_all(format!("{line}\n").as_bytes());
            }
        });
        Some(a)
    }

    /// что назвать в SUDO_ASKPASS
    fn program(&self) -> PathBuf {
        self.dir.join(ASKPASS_NAME)
    }
}

impl Drop for Askpass {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // нить ждёт в accept: подключение её будит, и она видит stop
        let _ = UnixStream::connect(self.dir.join("askpass"));
        if let Ok(mut p) = self.pass.lock() {
            yami_px::ui::wipe(&mut p);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// откуда ставить: папка первой установки (её записал установщик), иначе своя копия
pub fn source_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("YAMI_SRC") {
        return PathBuf::from(d);
    }
    let state = yami_px::paths::state_dir().join("source");
    std::fs::read_to_string(state).ok().map(|s| PathBuf::from(s.trim())).filter(|p| p.join("install.sh").exists()).unwrap_or_else(|| yami_px::paths::data_dir().join("yami/src"))
}

/// ставить есть чем: исходники на месте или git, чтобы их взять
pub fn can_install() -> bool {
    crate::update::enabled() && (source_dir().join("install.sh").exists() || crate::proc::have("git"))
}

/// есть ли sudo: через doas окну пароль не передать, пакеты тогда — одной командой потом
pub fn has_sudo() -> bool {
    crate::proc::have("sudo")
}

/// sudo спросит пароль? false — NOPASSWD или уже root
pub fn needs_password() -> bool {
    has_sudo() && !Command::new("sudo").args(["-n", "true"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// [3/8] шаг
fn step_of(s: &str) -> Option<(u32, u32, String)> {
    let i = s.find('[')?;
    let (nums, text) = s[i + 1..].split_once(']')?;
    let (k, n) = nums.split_once('/')?;
    Some((k.trim().parse().ok()?, n.trim().parse().ok()?, text.trim().to_string()))
}

/// строка без управляющих последовательностей терминала
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
        } else if c != '\r' {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// поставить ровно этот набор модулей (снятые уходят); ход — в Msg::Install
pub fn start(modules: Vec<String>, lang: String, mut password: String, tx: calloop::channel::Sender<Msg>) {
    let send = move |e: Ev| {
        let _ = tx.send(Msg::Install(e));
    };
    std::thread::spawn(move || {
        let src = source_dir();
        let script = src.join("install.sh");
        // первая установка шла из папки, которой уже нет (/tmp после перезагрузки): своя копия заново
        if !script.exists() {
            send(Ev::Line("git clone yami".into()));
            let ok = Command::new("git").args(["clone", "-q", "--filter=blob:none", REPO]).arg(&src).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
            if !ok {
                return send(Ev::Done(Err(crate::tf!("нет исходников yami и не удалось их скачать: проверь интернет", "the yami sources are gone and could not be downloaded: check the internet", "os fontes do yami sumiram e não foi possível baixá-los: verifique a internet"))));
            }
        }
        let mut c = Command::new("bash");
        c.arg(&script).args(["--apply", &modules.join(" ")]);
        if !lang.is_empty() {
            c.args(["--lang", &lang]);
        }
        c.current_dir(&src).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        // живёт до конца установки: нужен и шагам после сборки (экран входа)
        let mut secret = None;
        if password.is_empty() {
            if needs_password() || !has_sudo() {
                c.arg("--no-sudo");
            }
        } else {
            let got = Askpass::start(&password);
            yami_px::ui::wipe(&mut password);
            let Some(a) = got else { return send(Ev::Done(Err("askpass".into()))) };
            let check = Command::new("sudo").args(["-k", "-A", "true"]).env("SUDO_ASKPASS", a.program()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
            if !check.is_ok_and(|s| s.success()) {
                return send(Ev::WrongPassword);
            }
            c.env("SUDO_ASKPASS", a.program());
            secret = Some(a);
        }
        let mut child = match crate::proc::owned(&mut c).spawn() {
            Ok(ch) => ch,
            Err(e) => return send(Ev::Done(Err(e.to_string()))),
        };
        let (Some(out), err) = (child.stdout.take(), child.stderr.take()) else { return };
        if let Some(err) = err {
            std::thread::spawn(move || for _ in BufReader::new(err).lines() {});
        }
        // строки «! …» установщика: почему не вышло, понятными словами
        let mut why: Vec<String> = vec![];
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let l = plain_line(&line);
            if l.is_empty() {
                continue;
            }
            if let Some(w) = l.strip_prefix('!') {
                why.push(w.trim().to_string());
            }
            match step_of(&l) {
                Some((k, n, t)) => send(Ev::Step(k, n, t)),
                None => send(Ev::Line(l)),
            }
        }
        let st = child.wait();
        drop(secret);
        let log = yami_px::paths::pretty(&yami_px::paths::cache_dir().join("yami/install.log"));
        send(Ev::Done(match st {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => {
                let text: Vec<String> = why.into_iter().rev().take(2).rev().collect();
                Err(if text.is_empty() { format!("{}: {log}", crate::tf!("подробности", "details", "detalhes")) } else { format!("{} · {log}", text.join(" · ")) })
            }
            Err(e) => Err(e.to_string()),
        }));
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn steps_and_lines() {
        assert_eq!(super::step_of("[3/8] yami-files"), Some((3, 8, "yami-files".into())));
        assert_eq!(super::step_of("Compiling yami-px"), None);
        assert_eq!(super::plain_line("\u{1b}[1mbold\u{1b}[0m text\r"), "bold text");
    }

    /// пароль отдаёт только запуск под именем ссылки, и всё исчезает вместе с установкой
    #[test]
    fn askpass_lives_in_its_folder() {
        let dir;
        {
            let a = super::Askpass::start("secret").unwrap();
            dir = a.dir.clone();
            assert!(a.program().ends_with(super::ASKPASS_NAME));
            assert!(std::os::unix::fs::MetadataExt::mode(&std::fs::metadata(&dir).unwrap()) & 0o077 == 0);
            let s = super::UnixStream::connect(dir.join("askpass")).unwrap();
            let mut line = String::new();
            std::io::BufRead::read_line(&mut std::io::BufReader::new(s), &mut line).unwrap();
            assert_eq!(line, "secret\n");
        }
        assert!(!dir.exists());
    }
}
