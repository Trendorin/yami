// Кто держит камеру, для индикатора приватности (privacy.rs): inotify на /dev/video* будит нить при
// каждом открытии и закрытии (опроса нет), тогда она смотрит, чьи процессы держат эти файлы. Камеру,
// подключённую позже, нить замечает, пересматривая /dev раз в 30 с. Нить живёт, только пока индикатор
// камеры в баре: убрали — она выходит при следующем пробуждении. Идея из форка yami-lite.

use crate::app::Msg;
use calloop::channel::Sender;
use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};

static WANT: AtomicBool = AtomicBool::new(false);
static RUNNING: AtomicBool = AtomicBool::new(false);

fn devices() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir("/dev").into_iter().flatten().flatten().map(|e| e.path().to_string_lossy().into_owned()).filter(|p| p.starts_with("/dev/video")).collect();
    v.sort();
    v
}

/// имена процессов, у которых открыта камера (чужие /proc не покажет — видны только свои программы).
/// Камера через PipeWire (портал Camera) открыта самим pipewire: тогда имя — «pipewire»
fn users() -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for e in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(fds) = std::fs::read_dir(format!("/proc/{pid}/fd")) else { continue };
        if fds.flatten().any(|f| std::fs::read_link(f.path()).is_ok_and(|t| t.as_os_str().as_encoded_bytes().starts_with(b"/dev/video"))) {
            let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).unwrap_or_default().trim().to_string();
            if !comm.is_empty() && !out.contains(&comm) {
                out.push(comm);
            }
        }
    }
    out
}

/// индикатор камеры в баре (или нет); нить запускается по первому требованию
pub fn want(on: bool, tx: &Sender<Msg>) {
    WANT.store(on, Ordering::SeqCst);
    if !on || RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let tx = tx.clone();
    let ok = std::thread::Builder::new().name("camera".into()).spawn(move || {
        // want(true) между выходом из watch и снятием RUNNING вернулся бы ни с чем: нить забирает его сама
        while watch(&tx) {
            RUNNING.store(false, Ordering::SeqCst);
            if !WANT.load(Ordering::SeqCst) || RUNNING.swap(true, Ordering::SeqCst) {
                let _ = tx.send(Msg::Camera(vec![]));
                return;
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
    });
    if ok.is_err() {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// true — вышла, потому что больше не нужна; false — inotify нет или шелл уходит
fn watch(tx: &Sender<Msg>) -> bool {
    // SAFETY: inotify — обычный fd этой нити; читается и закрывается только здесь
    let fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
    if fd < 0 {
        return false;
    }
    let mut watched: Vec<String> = vec![];
    let mut last: Option<Vec<String>> = None;
    let mut buf = [0u8; 4096];
    while WANT.load(Ordering::Relaxed) {
        let now = devices();
        if now != watched {
            for d in &now {
                if let Ok(c) = CString::new(d.as_str()) {
                    unsafe { libc::inotify_add_watch(fd, c.as_ptr(), libc::IN_OPEN | libc::IN_CLOSE_WRITE | libc::IN_CLOSE_NOWRITE) };
                }
            }
            watched = now;
        }
        let u = if watched.is_empty() { vec![] } else { users() };
        if last.as_ref() != Some(&u) {
            if tx.send(Msg::Camera(u.clone())).is_err() {
                unsafe { libc::close(fd) };
                return false;
            }
            last = Some(u);
        }
        // ждать события камеры не дольше 30 с (потом пересмотреть /dev и спросить, нужна ли нить)
        let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut p, 1, 30_000) } > 0 {
            unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            // открытие и закрытие идут пачкой: дать программе договорить
            std::thread::sleep(std::time::Duration::from_millis(150));
            let mut p = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            while unsafe { libc::poll(&mut p, 1, 0) } > 0 {
                unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            }
        }
    }
    unsafe { libc::close(fd) };
    true
}
