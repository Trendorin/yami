// Один экземпляр на сеанс и замена на месте: `--reload` шлёт SIGHUP, процесс делает exec своего
// бинаря, как только не занят (нет открытого окна пароля и идущего показа экрана). Показ экрана
// нельзя просто убить: xdg-desktop-portal запоминает источники у живого бэкенда.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::os::unix::io::AsRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicUsize, Ordering};

/// занято: запросов и сеансов показа экрана, при которых нельзя перезапускаться
pub static BUSY: AtomicUsize = AtomicUsize::new(0);

pub struct Busy;
impl Busy {
    pub fn take() -> Busy {
        BUSY.fetch_add(1, Ordering::SeqCst);
        Busy
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.fetch_sub(1, Ordering::SeqCst);
    }
}

fn lock_path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join("yami-agents.lock"))
}

/// замок экземпляра; None — уже запущен
pub fn single_instance() -> Option<Option<File>> {
    let Some(p) = lock_path() else { return Some(None) };
    let Ok(mut f) = OpenOptions::new().create(true).truncate(false).read(true).write(true).open(p) else { return Some(None) };
    // SAFETY: flock на своём открытом файле; снимается при закрытии
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return None;
    }
    let _ = f.set_len(0);
    let _ = f.rewind();
    let _ = write!(f, "{}", std::process::id());
    Some(Some(f))
}

/// pid запущенного экземпляра: файл замка занят
pub fn running_pid() -> Option<i32> {
    let mut f = OpenOptions::new().read(true).write(true).open(lock_path()?).ok()?;
    // SAFETY: проба на своём открытом файле
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return None;
    }
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    s.trim().parse().ok()
}

pub fn signal() -> i32 {
    match running_pid() {
        Some(pid) => {
            // SAFETY: простой сигнал процессу из своего же файла замка
            unsafe { libc::kill(pid, libc::SIGHUP) };
            0
        }
        // не запущен: запуск и есть перечитывание. Служба живёт дольше того, кто её позвал: чужие
        // потоки она не держит, иначе читающий вывод установщик ждал бы её конца вечно
        None => match std::process::Command::new(yami_px::paths::self_exe()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0).spawn() {
            Ok(_) => 0,
            Err(_) => 1,
        },
    }
}

fn hangup_set() -> libc::sigset_t {
    // SAFETY: sigset_t — простая структура, sigemptyset заполняет её целиком
    unsafe {
        let mut s: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut s);
        libc::sigaddset(&mut s, libc::SIGHUP);
        s
    }
}

pub fn block_hangup() {
    let s = hangup_set();
    // SAFETY: маска наследуется нитями, которые запустятся позже
    unsafe { libc::pthread_sigmask(libc::SIG_BLOCK, &s, std::ptr::null_mut()) };
}

/// отдельная нить ждёт SIGHUP и сообщает об этом f
pub fn watch(f: impl Fn() + Send + 'static) {
    std::thread::spawn(move || {
        let s = hangup_set();
        loop {
            let mut sig = 0;
            // SAFETY: s и sig живут дольше вызова
            if unsafe { libc::sigwait(&s, &mut sig) } == 0 && sig == libc::SIGHUP {
                f();
            }
        }
    });
}

/// заменить процесс новым бинарём (маска и дескрипторы: замок закроется, новый образ возьмёт его заново)
pub fn exec_self() {
    use std::os::unix::process::CommandExt;
    let s = hangup_set();
    // SAFETY: снять блокировку SIGHUP, чтобы маска не досталась новому образу
    unsafe { libc::pthread_sigmask(libc::SIG_UNBLOCK, &s, std::ptr::null_mut()) };
    let e = std::process::Command::new(yami_px::paths::self_exe()).exec();
    eprintln!("yami-agents: reload: {e}");
}
