// Автоподключение устройств, как в GNOME и KDE: флешки, внешние диски и карты памяти, диски
// CD/DVD, образы дисков, телефоны (Android: MTP), фотоаппараты и телефоны в режиме PTP (gphoto2),
// iPhone и iPad (AFC). Шелл слушает события udev через netlink — без опроса: фильтр в ядре
// пропускает только подсистемы «block» и «usb», так что заряд батареи и сеть шелл не будят.
// После события (и ещё дважды позже: телефону нужно время, iPhone ждёт «доверять») спрашиваем
// у gio, какие тома есть, и подключаем новые — те, что сам том велит подключать
// (should_automount: съёмные диски, образы, телефоны — да; разделы внутренних дисков — нет).
// Каждый том — один раз за появление: отключённое руками само обратно не подключается.
// Зашифрованные (LUKS, BitLocker) не трогаем: пароль спросит yami-files (--unlock).

use crate::app::{App, Msg};
use crate::notify;
use std::collections::{HashMap, HashSet};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// подсистемы, после которых стоит смотреть тома: хеши MurmurHash2 их имён (так их считает
/// libudev и кладёт в заголовок каждого сообщения; проверено на живых событиях)
const BLOCK: u32 = 0xf003_1db7;
const USB: u32 = 0x0577_c5e5;
/// «libudev» + магия заголовка (в сети — big-endian, BPF читает так же)
const MAGIC: u32 = 0xfeed_cafe;
const NETLINK_KOBJECT_UEVENT: i32 = 15;
const SO_ATTACH_FILTER: i32 = 26;
/// повторные взгляды после события: телефону и iPhone нужно время, пока gvfs их увидит
const AGAIN: [u64; 2] = [5, 12];
/// заблокированный телефон не отдаёт память: пробовать снова каждые RETRY секунд, TRIES раз
const RETRY: u64 = 6;
const TRIES: u8 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Disk,
    Disc,
    Phone,
    Camera,
}

#[derive(Clone, Debug, Default)]
pub struct Vol {
    pub name: String,
    monitor: String,
    /// /dev/sdb1, /dev/bus/usb/003/007
    pub dev: String,
    uuid: String,
    /// mtp://…, afc://…, cdda://…
    root: String,
    auto: bool,
    can_mount: bool,
    /// адрес подключения (file:///run/media/…, mtp://…), пусто — не подключён
    mount: String,
}

impl Vol {
    pub(crate) fn key(&self) -> String {
        [&self.uuid, &self.dev, &self.root, &self.name].iter().find(|s| !s.is_empty()).map(|s| s.to_string()).unwrap_or_default()
    }
    pub fn kind(&self) -> Kind {
        let m = self.monitor.to_ascii_lowercase();
        let scheme = self.root.split("://").next().unwrap_or("");
        if m.contains("mtp") || m.contains("afc") || scheme == "mtp" || scheme == "afc" {
            Kind::Phone
        } else if m.contains("gphoto2") || scheme == "gphoto2" {
            Kind::Camera
        } else if self.dev.starts_with("/dev/sr") || scheme == "cdda" {
            Kind::Disc
        } else {
            Kind::Disk
        }
    }
    /// телефоны и камеры подключаем всегда (их тома и есть «подключи меня»), диски — по слову тома
    fn wanted(&self) -> bool {
        self.mount.is_empty() && self.can_mount && (self.auto || self.kind() == Kind::Phone || self.kind() == Kind::Camera)
    }
}

#[derive(Default)]
pub struct Am {
    /// когда посмотреть тома; сколько повторов ещё осталось
    due: Option<Instant>,
    again: usize,
    /// тома, что уже были при прошлом взгляде; None — первого взгляда ещё не было
    seen: Option<HashSet<String>>,
    /// взгляд идёт в нити
    busy: bool,
    /// неудачные попытки по тому (телефон заблокирован): ещё раз, пока не кончатся
    tries: HashMap<String, u8>,
    /// таймер взгляда уже идёт
    armed: bool,
}

/// сокет событий udev с фильтром в ядре; None — нет netlink (контейнер) или ошибка
fn open() -> Option<OwnedFd> {
    // SAFETY: обычные вызовы сокетов; память структур — наша, на стеке, на время вызова
    unsafe {
        let fd = libc::socket(libc::AF_NETLINK, libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK, NETLINK_KOBJECT_UEVENT);
        if fd < 0 {
            return None;
        }
        let fd = OwnedFd::from_raw_fd(fd);
        let mut sa: libc::sockaddr_nl = std::mem::zeroed();
        sa.nl_family = libc::AF_NETLINK as u16;
        // группа 2 — события после udev: права на узлы и тома gvfs уже на месте
        sa.nl_groups = 2;
        if libc::bind(fd.as_raw_fd(), &sa as *const _ as *const libc::sockaddr, std::mem::size_of::<libc::sockaddr_nl>() as u32) < 0 {
            return None;
        }
        let f = |code: u16, jt: u8, jf: u8, k: u32| libc::sock_filter { code, jt, jf, k };
        // BPF: не libudev — мимо; подсистема block или usb — пропустить; прочее — мимо
        let (ld, jeq, ret) = (0x20, 0x15, 0x06);
        let prog = [f(ld, 0, 0, 8), f(jeq, 1, 0, MAGIC), f(ret, 0, 0, 0), f(ld, 0, 0, 24), f(jeq, 2, 0, BLOCK), f(jeq, 1, 0, USB), f(ret, 0, 0, 0), f(ret, 0, 0, 0xffff_ffff)];
        let fp = libc::sock_fprog { len: prog.len() as u16, filter: prog.as_ptr() as *mut _ };
        // без фильтра работает тоже, только будит чаще
        let _ = libc::setsockopt(fd.as_raw_fd(), libc::SOL_SOCKET, SO_ATTACH_FILTER, &fp as *const _ as *const libc::c_void, std::mem::size_of::<libc::sock_fprog>() as u32);
        Some(fd)
    }
}

/// вычерпать сокет: содержимое не нужно — событие лишь повод посмотреть тома
fn drain(fd: &OwnedFd) {
    let mut b = [0u8; 8192];
    // SAFETY: читаем в свой буфер его длины
    while unsafe { libc::recv(fd.as_raw_fd(), b.as_mut_ptr() as *mut libc::c_void, b.len(), libc::MSG_DONTWAIT) } > 0 {}
}

/// тип файловой системы раздела из базы udev (без lsblk): «crypto_LUKS», «vfat»…
fn fs_type(dev: &str) -> String {
    use std::os::unix::fs::MetadataExt;
    let Ok(m) = std::fs::metadata(dev) else { return String::new() };
    let rdev = m.rdev();
    let (maj, min) = (libc::major(rdev), libc::minor(rdev));
    let db = std::fs::read_to_string(format!("/run/udev/data/b{maj}:{min}")).unwrap_or_default();
    db.lines().find_map(|l| l.strip_prefix("E:ID_FS_TYPE=")).unwrap_or("").to_string()
}

fn locked(v: &Vol) -> bool {
    v.kind() == Kind::Disk && v.dev.starts_with("/dev/") && matches!(fs_type(&v.dev).as_str(), "crypto_LUKS" | "BitLocker")
}

fn gio() -> Command {
    let mut c = Command::new("gio");
    c.env("LC_ALL", "C").stdin(Stdio::null()).stderr(Stdio::piped()).stdout(Stdio::piped());
    c
}

/// тома gio: `gio mount -li` (тома дисков вложены в свои диски — по отступам)
pub fn scan() -> Vec<Vol> {
    let Ok(o) = gio().args(["mount", "-li"]).output() else { return vec![] };
    parse(&String::from_utf8_lossy(&o.stdout))
}

fn parse(text: &str) -> Vec<Vol> {
    let mut out: Vec<Vol> = vec![];
    // отступ строки тома; отступ строки его подключения (внутри неё — свойства подключения)
    let (mut vi, mut mi): (Option<usize>, Option<usize>) = (None, None);
    for l in text.lines() {
        let ind = l.len() - l.trim_start().len();
        let s = l.trim();
        if mi.is_some_and(|m| ind <= m) {
            mi = None;
        }
        if vi.is_some_and(|v| ind <= v) {
            vi = None;
            mi = None;
        }
        if let Some(name) = s.strip_prefix("Volume(").and_then(|r| r.split_once("): ")).map(|x| x.1) {
            out.push(Vol { name: name.to_string(), ..Default::default() });
            vi = Some(ind);
            continue;
        }
        let (Some(_), Some(v)) = (vi, out.last_mut()) else { continue };
        if let Some(rest) = s.strip_prefix("Mount(").and_then(|r| r.split_once("): ")).map(|x| x.1) {
            v.mount = rest.split_once(" -> ").map(|x| x.1.trim().to_string()).unwrap_or_default();
            mi = Some(ind);
            continue;
        }
        if mi.is_some() {
            continue;
        }
        let q = |x: &str| x.trim().trim_matches('\'').to_string();
        if let Some(t) = s.strip_prefix("Type: ") {
            v.monitor = t.to_string();
        } else if let Some(d) = s.strip_prefix("unix-device: ") {
            v.dev = q(d);
        } else if let Some(u) = s.strip_prefix("uuid: ").or_else(|| s.strip_prefix("uuid=")) {
            v.uuid = q(u);
        } else if let Some(r) = s.strip_prefix("activation_root=") {
            v.root = r.trim().to_string();
        } else if let Some(x) = s.strip_prefix("can_mount=") {
            v.can_mount = x.trim() == "1";
        } else if let Some(x) = s.strip_prefix("should_automount=") {
            v.auto = x.trim() == "1";
        }
    }
    out
}

/// локальная папка подключения: file:// — сам путь, gvfs — его FUSE-точка
fn local(uri: &str) -> Option<String> {
    if uri.starts_with("file://") {
        return Some(yami_px::icons::uri_path(uri)?.to_string_lossy().into_owned());
    }
    let o = gio().args(["info", "-a", "standard::type", "--", uri]).output().ok()?;
    String::from_utf8_lossy(&o.stdout).lines().find_map(|l| l.strip_prefix("local path: ").map(|p| p.trim().to_string()))
}

/// подключить том: адрес (телефоны, камеры, iPhone, аудио-CD) или раздел; ждём не дольше 40 с
/// (телефон ждёт разблокировки). Ok — локальная папка (если есть)
fn mount(v: &Vol) -> Result<Option<String>, String> {
    let mut c = gio();
    c.arg("mount");
    if v.root.is_empty() {
        c.args(["-d", &v.dev]);
    } else {
        c.args(["--", &v.root]);
    }
    c.stdout(Stdio::null());
    let mut ch = owned(&mut c).spawn().map_err(|_| crate::i18n::tr("нет gio (пакет glib2)").to_string())?;
    let t = Instant::now();
    let ok = loop {
        match ch.try_wait() {
            Ok(Some(s)) => break s.success(),
            Ok(None) if t.elapsed() < Duration::from_secs(40) => std::thread::sleep(Duration::from_millis(150)),
            _ => {
                let _ = ch.kill();
                let _ = ch.wait();
                break false;
            }
        }
    };
    let mut err = String::new();
    if let Some(mut e) = ch.stderr.take() {
        use std::io::Read;
        let _ = e.read_to_string(&mut err);
    }
    if !ok && !err.contains("already mounted") {
        let l = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim();
        let l = l.strip_prefix("gio: ").unwrap_or(l);
        return Err(l.rsplit_once(": ").map(|x| x.1).unwrap_or(l).to_string());
    }
    let k = v.key();
    let uri = scan().into_iter().find(|x| x.key() == k).map(|x| x.mount).filter(|m| !m.is_empty());
    Ok(uri.as_deref().and_then(local))
}

/// дочерний процесс умирает вместе с нитью, которая его ждёт (замена бинаря на месте)
fn owned(c: &mut Command) -> &mut Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: prctl безопасен между fork и exec
    unsafe {
        c.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        })
    }
}

/// заперт ли экран: шелл yami ставит метку; у остальных спрашиваем logind
fn session_locked() -> bool {
    if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR") {
        if std::path::PathBuf::from(d).join("yami-locked").exists() {
            return true;
        }
    }
    let Ok(id) = std::env::var("XDG_SESSION_ID") else { return false };
    let Ok(out) = Command::new("loginctl").args(["show-session", &id, "-p", "LockedHint", "--value"]).stdin(Stdio::null()).stderr(Stdio::null()).output() else { return false };
    String::from_utf8_lossy(&out.stdout).trim() == "yes"
}

/// игра на весь экран: новое окно поверх неё не открываем
fn fullscreen_in_front() -> bool {
    if std::env::var_os("NIRI_SOCKET").is_none() {
        return false;
    }
    let Ok(o) = Command::new("niri").args(["msg", "-j", "focused-window"]).stderr(Stdio::null()).output() else { return false };
    serde_json::from_slice::<serde_json::Value>(&o.stdout).is_ok_and(|v| v["is_fullscreen"].as_bool() == Some(true))
}

fn open_path(p: &str) {
    let _ = Command::new("xdg-open").arg(p).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map(|mut c| std::thread::spawn(move || c.wait()));
}

/// открыть зашифрованный диск и подключить: пароль через stdin udisksctl, не в argv и не на диск.
/// Ok — куда подключён, Err — почему нет
pub fn unlock(dev: &str, pass: &str) -> Result<String, String> {
    use std::io::Write;
    let mut ch = Command::new("udisksctl").args(["unlock", "--no-user-interaction", "-b", dev, "--key-file", "/dev/stdin"]).env("LC_ALL", "C").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|_| crate::i18n::tr("нет udisksctl (пакет udisks2)").to_string())?;
    if let Some(mut i) = ch.stdin.take() {
        let _ = i.write_all(pass.as_bytes());
    }
    let out = ch.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("").to_string());
    }
    // «Unlocked /dev/sdb1 as /dev/dm-3.»
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mapped = text.split(" as ").nth(1).map(|s| s.trim().trim_end_matches('.').to_string()).ok_or_else(|| text.clone())?;
    let m = Command::new("udisksctl").args(["mount", "--no-user-interaction", "-b", &mapped]).env("LC_ALL", "C").stdin(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !m.status.success() {
        return Err(String::from_utf8_lossy(&m.stderr).lines().last().unwrap_or("").to_string());
    }
    // «Mounted /dev/dm-3 at /run/media/user/disk»
    let t = String::from_utf8_lossy(&m.stdout).to_string();
    Ok(t.split(" at ").nth(1).map(|s| s.trim().trim_end_matches('.').to_string()).unwrap_or_default())
}

impl App {
    /// начать слежку, если она включена
    pub fn automount_sync(&mut self) {
        if !self.cfg.automount {
            return;
        }
        let Some(fd) = open() else { return };
        let src = calloop::generic::Generic::new(fd, calloop::Interest::READ, calloop::Mode::Level);
        let _ = self.handle.insert_source(src, |_, fd, app: &mut App| {
            drain(fd);
            app.automount_poke();
            Ok(calloop::PostAction::Continue)
        });
        // первый взгляд — чуть позже старта: запомнить, что уже вставлено
        self.automount_at(Instant::now() + Duration::from_secs(5));
    }

    /// подошло время — спросить gio в нити. Таймер живёт, только пока есть, чего ждать
    fn automount_at(&mut self, t: Instant) {
        self.am.due = Some(self.am.due.map_or(t, |d| d.min(t)));
        if !std::mem::replace(&mut self.am.armed, true) {
            self.arm(Duration::from_millis(300), |app| {
                app.automount_tick();
                let go_on = app.am.due.is_some();
                app.am.armed = go_on;
                go_on
            });
        }
    }

    /// событие udev: посмотреть тома через секунду (пачка событий — один взгляд) и ещё дважды
    fn automount_poke(&mut self) {
        self.am.due = None;
        self.am.again = AGAIN.len();
        self.automount_at(Instant::now() + Duration::from_millis(1200));
    }

    fn automount_tick(&mut self) {
        let am = &mut self.am;
        if am.busy || !am.due.is_some_and(|d| Instant::now() >= d) {
            return;
        }
        am.due = None;
        if am.again > 0 {
            let i = AGAIN.len() - am.again;
            am.again -= 1;
            am.due = Some(Instant::now() + Duration::from_secs(AGAIN[i]));
        }
        am.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let locked = session_locked();
            let _ = tx.send(Msg::Volumes(scan(), locked));
        });
    }

    /// тома пришли: новые — подключить; первый взгляд в сеансе подключает и то, что было вставлено
    /// заранее (как GNOME), после перезапуска службы — только запоминает
    pub(crate) fn automount_volumes(&mut self, vols: Vec<Vol>, screen_locked: bool) {
        let restarted = self.restarted;
        let am = &mut self.am;
        am.busy = false;
        let first = am.seen.is_none();
        let seen = am.seen.take().unwrap_or_default();
        // экран заблокирован: чужую флешку без хозяина не подключаем (драйвер ФС — мишень для
        // кривого образа, как и в GNOME). Новые тома не запоминаем — их подключит взгляд после
        // разблокировки: пока заперто, смотрим раз в 5 с
        if screen_locked && !(first && restarted) {
            am.seen = Some(if first { vols.iter().filter(|v| !v.mount.is_empty()).map(Vol::key).collect() } else { seen.into_iter().filter(|k| vols.iter().any(|v| v.key() == *k)).collect() });
            self.automount_at(Instant::now() + Duration::from_secs(5));
            return;
        }
        am.seen = Some(vols.iter().map(Vol::key).collect());
        // вынутые тома забываем вместе с их попытками
        am.tries.retain(|k, _| vols.iter().any(|v| v.key() == *k));
        if first && restarted {
            return;
        }
        for v in vols.into_iter().filter(|v| !seen.contains(&v.key())) {
            if locked(&v) && v.mount.is_empty() {
                let title = crate::tf!("зашифрованный диск: {}", "encrypted drive: {}", "disco criptografado: {}", v.name);
                self.note(title, crate::i18n::tr("нажми, чтобы ввести пароль").to_string(), "drive-removable-media", format!("unlock:{} {}", v.dev, v.name));
                continue;
            }
            if !v.wanted() {
                continue;
            }
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let res = mount(&v);
                let _ = tx.send(Msg::AutoMounted(v, res));
            });
        }
    }

    /// подключили (или нет): открыть в файлах или показать карточку
    pub(crate) fn automount_done(&mut self, v: Vol, res: Result<Option<String>, String>) {
        let icon = match v.kind() {
            Kind::Phone => "phone",
            Kind::Camera => "camera-photo",
            Kind::Disc => "media-optical",
            Kind::Disk => "drive-removable-media",
        };
        match res {
            Ok(path) => {
                self.am.tries.remove(&v.key());
                let path = path.unwrap_or_default();
                if self.cfg.automount_open && !path.is_empty() && !fullscreen_in_front() {
                    open_path(&path);
                    return;
                }
                let body = if path.is_empty() { String::new() } else { yami_px::paths::pretty(std::path::Path::new(&path)) };
                let summary = match v.kind() {
                    Kind::Phone => crate::tf!("телефон подключён: {}", "phone connected: {}", "telefone conectado: {}", v.name),
                    Kind::Camera => crate::tf!("камера подключена: {}", "camera connected: {}", "câmera conectada: {}", v.name),
                    _ => crate::tf!("подключено: {}", "mounted: {}", "montado: {}", v.name),
                };
                self.note(summary, body, icon, path);
            }
            Err(e) => {
                // телефон или камера заблокированы: том забываем, следующий взгляд попробует снова;
                // карточка — только когда попытки кончились
                let am = &mut self.am;
                let k = v.key();
                let n = am.tries.entry(k.clone()).or_insert(0);
                *n += 1;
                if matches!(v.kind(), Kind::Phone | Kind::Camera) && *n < TRIES {
                    if let Some(s) = am.seen.as_mut() {
                        s.remove(&k);
                    }
                    self.automount_at(Instant::now() + Duration::from_secs(RETRY));
                    return;
                }
                // телефон в режиме «только зарядка» или так и не разблокирован: подсказка вместо ошибки gvfs
                let body = if v.kind() == Kind::Phone { format!("{e} · {}", crate::i18n::tr("разблокируй телефон и выбери «передача файлов»")) } else { e };
                self.note(crate::tf!("не подключается: {}", "could not mount: {}", "não foi possível montar: {}", v.name), body, icon, String::new());
            }
        }
    }

    /// уведомление; open — что открыть по кнопке (путь или unlock:…)
    fn note(&mut self, summary: String, body: String, icon: &'static str, open: String) {
        let tx = self.tx.clone();
        let label = crate::i18n::tr("открыть").to_string();
        std::thread::spawn(move || {
            let has = !open.is_empty();
            if notify::send(&summary, &body, icon, has.then_some(label.as_str()), 120) && has {
                let _ = tx.send(Msg::Open(open));
            }
        });
    }

    /// кнопка уведомления: открыть папку или спросить пароль диска
    pub(crate) fn note_open(&mut self, what: String) {
        let Some(rest) = what.strip_prefix("unlock:") else { return open_path(&what) };
        let (dev, name) = rest.split_once(' ').unwrap_or((rest, rest));
        let cookie = format!("unlock-{dev}");
        if self.auth.iter().any(|r| r.cookie == cookie) {
            return;
        }
        self.auth.push(crate::polkit::Req { cookie, message: crate::tf!("зашифрованный диск: {}", "encrypted drive: {}", "disco criptografado: {}", name), action: dev.to_string(), user: String::new(), prompt: String::new(), echo: false, info: String::new(), fails: 0, busy: false, field: Default::default(), shake: None, unlock: Some(dev.to_string()), _guard: crate::reload::Busy::take() });
        self.sync_panel();
        self.dirty();
    }

    /// udisks ответил про зашифрованный диск
    pub(crate) fn on_unlocked(&mut self, message: String, res: Result<String, String>) {
        let Some(i) = self.auth.iter().position(|r| r.message == message && r.unlock.is_some()) else { return };
        match res {
            Ok(path) => {
                self.auth.remove(i);
                self.sync_panel();
                if self.cfg.automount_open && !path.is_empty() && !fullscreen_in_front() {
                    open_path(&path);
                } else {
                    let body = yami_px::paths::pretty(std::path::Path::new(&path));
                    self.note(crate::tf!("подключено: {}", "mounted: {}", "montado: {}", message), body, "drive-removable-media", path);
                }
            }
            Err(e) => {
                let r = &mut self.auth[i];
                r.busy = false;
                r.fails += 1;
                r.info = e;
                r.shake = Some(Instant::now());
            }
        }
        self.dirty();
    }
}

#[cfg(test)]
mod t {
    use super::*;

    fn murmur2(b: &[u8]) -> u32 {
        let m = 0x5bd1_e995u32;
        let mut h = b.len() as u32;
        let mut c = b.chunks_exact(4);
        for k in &mut c {
            let mut k = u32::from_le_bytes([k[0], k[1], k[2], k[3]]).wrapping_mul(m);
            k ^= k >> 24;
            h = h.wrapping_mul(m) ^ k.wrapping_mul(m);
        }
        let r = c.remainder();
        if !r.is_empty() {
            for (i, x) in r.iter().enumerate() {
                h ^= (*x as u32) << (8 * i);
            }
            h = h.wrapping_mul(m);
        }
        h ^= h >> 13;
        h = h.wrapping_mul(m);
        h ^ (h >> 15)
    }

    #[test]
    fn hashes() {
        assert_eq!(murmur2(b"block"), BLOCK);
        assert_eq!(murmur2(b"usb"), USB);
        // сверено с заголовками живых событий udev
        assert_eq!(murmur2(b"misc"), 0x4b1a_4e75);
        assert_eq!(murmur2(b"bdi"), 0x2a18_84a1);
    }

    #[test]
    fn volumes() {
        let t = "Drive(0): SAMSUNG\n  Type: GProxyDrive (GProxyVolumeMonitorUDisks2)\n  Volume(0): root\n    Type: GProxyVolume (GProxyVolumeMonitorUDisks2)\n    ids:\n     unix-device: '/dev/nvme0n1p2'\n    can_mount=1\n    should_automount=0\n    Mount(0): root -> file:///\n      Type: GProxyMount (GProxyVolumeMonitorUDisks2)\nVolume(0): YAMITEST\n  Type: GProxyVolume (GProxyVolumeMonitorUDisks2)\n  ids:\n   unix-device: '/dev/loop0'\n   uuid: 'C397-BAE0'\n  can_mount=1\n  should_automount=1\nVolume(1): Pixel 8\n  Type: GProxyVolume (GProxyVolumeMonitorMTP)\n  ids:\n   unix-device: '/dev/bus/usb/003/007'\n  activation_root=mtp://Google_Pixel_8_3A1/\n  can_mount=1\n  should_automount=1\n";
        let v = parse(t);
        assert_eq!(v.len(), 3);
        assert_eq!((v[0].dev.as_str(), v[0].mount.as_str(), v[0].wanted()), ("/dev/nvme0n1p2", "file:///", false));
        assert_eq!((v[1].key().as_str(), v[1].kind(), v[1].wanted()), ("C397-BAE0", Kind::Disk, true));
        assert_eq!((v[2].root.as_str(), v[2].kind(), v[2].wanted()), ("mtp://Google_Pixel_8_3A1/", Kind::Phone, true));
    }
}
