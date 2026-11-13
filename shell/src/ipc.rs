// Команды шеллу: `yami-shell msg launcher` → сокет $XDG_RUNTIME_DIR/yami-shell.sock.
// Ими пользуются бинды niri. Второй экземпляр шелла сюда же и упирается — и выходит.
// Безопасность: сокет 0600, говорим только с процессами своего пользователя (SO_PEERCRED),
// строка не длиннее 4 КБ и ждём её не дольше 2 с — зависший клиент не заткнёт шелл.

use crate::app::{Modal, Msg, SKind};
use crate::config::Settings;
use calloop::channel::Sender;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

pub fn path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    PathBuf::from(dir).join("yami-shell.sock")
}

/// команда шеллу; true — шелл её принял (ответил). Ответа ждём не дольше 5 с: зависший шелл
/// не должен вешать установщик и `yami update`
pub fn send(cmd: &str) -> bool {
    match UnixStream::connect(path()) {
        Ok(mut s) => {
            let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let _ = s.set_write_timeout(Some(std::time::Duration::from_secs(5)));
            if writeln!(s, "{cmd}").is_err() {
                return false;
            }
            let mut r = String::new();
            BufReader::new(s).read_line(&mut r).map(|n| n > 0).unwrap_or(false)
        }
        Err(_) => false,
    }
}

pub fn serve(tx: Sender<Msg>) {
    let p = path();
    if UnixStream::connect(&p).is_ok() {
        eprintln!("{}", crate::tf!("yami-shell уже запущен", "yami-shell is already running", "yami-shell já está em execução"));
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&p);
    let Ok(l) = UnixListener::bind(&p) else {
        eprintln!("{}", crate::tf!("ipc: не создать {}", "ipc: cannot create {}", "ipc: não foi possível criar {}", p.display()));
        return;
    };
    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
    std::thread::Builder::new()
        .name("ipc".into())
        .spawn(move || {
            for s in l.incoming().flatten() {
                if !same_user(&s) {
                    continue;
                }
                let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new((&s).take(4096)).read_line(&mut line).is_err() {
                    continue;
                }
                let _ = s.set_read_timeout(None);
                let line = line.trim();
                if !line.is_empty() {
                    let _ = tx.send(Msg::Ipc(line.to_string()));
                }
                let _ = (&s).write_all(b"ok\n");
            }
        })
        .ok();
}

/// собеседник — процесс того же пользователя
fn same_user(s: &UnixStream) -> bool {
    let mut cred = libc::ucred { pid: 0, uid: u32::MAX, gid: 0 };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: getsockopt пишет не больше len байт в cred
    let r = unsafe { libc::getsockopt(s.as_raw_fd(), libc::SOL_SOCKET, libc::SO_PEERCRED, &mut cred as *mut _ as *mut libc::c_void, &mut len) };
    r == 0 && cred.uid == unsafe { libc::getuid() }
}

impl crate::app::App {
    // ── команды (IPC) ───────────────────────────────────────────────────────
    pub fn ipc(&mut self, cmd: &str) {
        let trimmed = cmd.trim();
        if trimmed == "launcher" {
            self.toggle_modal(Modal::Launcher, false);
            return;
        }
        if let Some(rest) = trimmed.strip_prefix("launcher") {
            let q = rest.trim();
            if q.is_empty() {
                self.toggle_modal(Modal::Launcher, false);
            } else {
                self.open_launcher_query(q);
            }
            return;
        }
        match trimmed {
            "night" => self.night_toggle(),
            "clipboard" => self.toggle_modal(Modal::Launcher, true),
            "power" => self.toggle_modal(Modal::Power, false),
            "settings" => self.toggle_modal(Modal::Settings, false),
            "walls" => self.walls_open(false),
            "welcome" => self.welcome_open("hello"),
            c if c.starts_with("welcome ") => self.welcome_open(c["welcome ".len()..].trim()),
            "lock" => self.lock_now(),
            "bar" => self.toggle_bar(),
            "stacking" => {
                let on = !self.st.niri.stacking;
                self.st.niri.set_stacking(on);
                self.st.set.stacking = on;
                // фокус за мышью есть только в ленте (keys::kdl)
                crate::keys::write(&self.st.set);
                self.sync_titles();
                self.dirty_all();
            }
            "minimize" => {
                if let Some(id) = self.st.niri.focused_win().map(|w| w.id) {
                    self.st.niri.minimize(id);
                }
            }
            "restore" => self.st.niri.restore_last(),
            "edit" => {
                self.close_all();
                self.set_editing(!self.st.editing);
            }
            "reload" => {
                self.st.set = Settings::load();
                self.st.apps = crate::icons::load_apps();
                self.settings_changed();
            }
            "quit" => self.quit = true,
            // «жив ли шелл» для install.sh и deploy.sh: ответ сокета, ничего не делает
            "ping" => {}
            "restart" => self.restart = true,
            c if self.update_ipc(c) => {}
            // медиаклавиши и бар: команда показанному плееру; player — другой плеер (#24)
            c if c.starts_with("media ") => match c[6..].trim() {
                "player" => self.media_step(1),
                a @ ("play-pause" | "play" | "pause" | "stop" | "next" | "previous") => self.media_ctl(a),
                _ => {}
            },
            c if c.starts_with("settings ") => self.open_settings_tab(c[9..].trim()),
            // снимает отдельная программа: второй запуск закрывает открытый оверлей (yami-shot)
            c if c == "screenshot" || c.starts_with("screenshot ") => {
                if self.st.locked {
                    return;
                }
                if !crate::proc::have("yami-shot") {
                    self.toast_note("скриншоты не установлены", "модуль yami-shot ставится из первой настройки: yami setup");
                    return;
                }
                self.close_all();
                let mut a = vec!["yami-shot".to_string()];
                let mut w = c.split_whitespace().skip(1);
                a.push(w.next().unwrap_or("area").to_string());
                if let Some(d) = w.next() {
                    a.extend(["--delay".to_string(), d.to_string()]);
                }
                crate::app::App::spawn(&a);
            }
            // следующий профиль питания (для хоткея)
            "profile" => self.prof_step(1),
            c if c.starts_with("popup ") => {
                let kind = crate::popups::KINDS.iter().copied().find(|k| *k == &c[6..]).unwrap_or("sys");
                // из терминала или хоткея: у правого (бокового — у нижнего) конца бара
                let out = self.focus_out();
                let (ow, oh) = self.output_size(&out);
                let side = crate::bar::geo(&self.st.set, if crate::bar::vert(&self.st.set) { oh } else { ow }).1;
                self.open_popup(kind, &out, (ow - side - 260, oh - side - 260));
            }
            // свои обои: путь; без пути — пиксельные из палитры
            c if c == "wallpaper" || c.starts_with("wallpaper ") => self.set_wall(&c["wallpaper".len()..]),
            c if c == "curtain" || c.starts_with("curtain ") => self.curtain_cmd(&c["curtain".len()..]),
            "cast-hide" => self.cast_pick(),
            _ => {}
        }
    }

    /// настройки на вкладке (`yami open settings:keys`, карточка обновления)
    pub fn open_settings_tab(&mut self, tab: &str) {
        let Some(t) = crate::settings::find(tab).map(|x| x.id) else { return };
        self.st.stab = t;
        if self.st.modal != Some(Modal::Settings) {
            self.toggle_modal(Modal::Settings, false);
        } else {
            self.tab_probe();
        }
        self.dirty(&SKind::Modal);
    }
}
